#![feature(seek_stream_len)]

mod fio;

use std::io::{Seek, SeekFrom, Write, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::fs::{OpenOptions};
use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use std::borrow::{Borrow, BorrowMut};
use crate::fio::MetadataSpace;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

static mut TABLE_LOCATION: Option<u64> = None;

const METADATA_SPACE_SIZE: u64 = 10;
const DEFAULT_ACCESSES_PER_SHIFT: u64 = 500;
const MAGIC_IDENTIFIER: u64 = 0x8d2765dd2bc8bf74;

///Proof of concept demonstration of STFS (Shifting table filesystem)

#[derive(Serialize, Deserialize)]
struct STFSFileMetadata {
    _start: u64,
    _len: u64,
    _flags: u16,
    _modified: SystemTime,
    _accessed: SystemTime,
    _created: SystemTime,
    _path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct ShiftingTable {
    ///This value keeps track of how many table read/writes are allowed before a shift is initiated
    _accesses_left: u64,
    ///Maximum number of read/writes to the table before a shift is initiated
    _accesses_per_shift: u64,
    ///Size, in sectors, of the table
    _table_size: u64,
    ///Magic constant to verify table. Not sure if this is really needed
    _magic: u64,
    ///Metadata for the one file on the filesystem
    _files_data: Vec<STFSFileMetadata>,

}

impl ShiftingTable {

    fn new() -> Self {
        let mut s = Self {
            _accesses_left: DEFAULT_ACCESSES_PER_SHIFT,
            _accesses_per_shift: DEFAULT_ACCESSES_PER_SHIFT,
            _table_size: 0,
            _magic: MAGIC_IDENTIFIER,
            _files_data: Vec::new(),
        };

        s.set_table_size();

        s
    }

    fn set_table_size(& mut self) {
        self._table_size = (bincode::serialize(&self).unwrap().len() as f64 / 496.0f64).ceil() as u64
    }
}

///We wrap the following functions as multithreaded acces is not a problem in this project
fn set_table_location(location: u64) {
    unsafe {
        TABLE_LOCATION = Some(location);
    }
}

fn get_table_location() -> u64 {
    unsafe {
        TABLE_LOCATION.unwrap()
    }
}

/// Create a fake storage media as a file
fn create<P: AsRef<Path>>(media: P, size: u64) -> Result<()> {

    let fp  = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(media)?;

    fp.set_len(size)?;

    Ok(())
}

/// Write the STFS table to a specific sector
fn write_table<S: Read + Write + Seek>(stream: S, location: u64, table: & ShiftingTable) -> Result<()> {

    let writer = MetadataSpace::new(stream, location);

    bincode::serialize_into(writer, table)?;

    Ok(())

}

fn read_table<S: Read + Write + Seek>(stream: S, location: u64) -> Result<ShiftingTable> {

    let reader = MetadataSpace::new(stream, location);

    Ok(bincode::deserialize_from(reader)?)
}

/// Format the media as STFS
fn format<S: Read + Write + Seek>(mut stream: S, metadata_space_size: u64) -> Result<()> {


    let sector_count = stream.stream_len()? / 512;

    if METADATA_SPACE_SIZE >= sector_count {
        return Err(Box::new(std::io::Error::new(ErrorKind::Other, "Storage media is too small")));
    }

    //Before we create a table, we must lay an initial trail across the entire metadata space
    // Putting down an initial trail allows the binary search algorithm to find the table
    // even if the table hasn't yet done a complete pass of the metadata space

    //Iterate over all sectors in metadata space and lay the initial trail which goes from 0 to METADATA_SPACE_SIZE-1
    for i in 0..metadata_space_size {
        let step_number_start = i * 512 + 496;

        stream.seek(SeekFrom::Start(step_number_start))?;

        bincode::serialize_into(& mut stream, &(i as u128))?;
    }

    // Writing the table to the beginning, we must include the new trail at METADATA_SPACE_SIZE.
    // So by this point, say the METADATA_SPACE_SIZE is 10. We should have a trail that looks like
    // 10 1 2 3 4 5 6 7 8 9

    stream.seek(SeekFrom::Start(496))?;

    bincode::serialize_into(& mut stream, &(METADATA_SPACE_SIZE as u128))?;

    let table = ShiftingTable::new();

    write_table(stream, 0, & table)?;

    Ok(())
}

/// Search for the table
fn search<S: Read + Write + Seek>(mut stream: S) -> Result<u64> {

    let mut start = 0;

    let mut end = METADATA_SPACE_SIZE-1;

    loop {

        stream.seek(SeekFrom::Start(start * 512 + 496))?;
        let start_step: u128 = bincode::deserialize_from(& mut stream)?;

        let middle = ((start + end) as f64 / 2.0f64).floor() as u64;
        stream.seek(SeekFrom::Start(middle * 512 + 496))?;

        let middle_step: u128 = bincode::deserialize_from(& mut stream)?;

        if start_step > middle_step {
            end = middle;
        } else if start_step < middle_step {
            start = middle;
        } else {
            return Ok(start);
        }
    }
}

///Equivalent to the FUSE init function
fn initialise<S: Read + Write + Seek>(stream: S) -> Result<()> {
    //When the FS is mounted, we manually search for the table then store the location in memory.
    //When we shift the table, se keep track of this in memory to avoid having to search each time

    set_table_location(search(stream)?);

    Ok(())
}

///Shift the table by one sector, updating metadata and leaving behind a trail. (sectors left behind contain zeros in the first 496 bytes
fn shift_table<S: Read + Write + Seek>(mut stream: S) -> Result<()> {


    //Get location
    let table_location = get_table_location();

    //Read the table
    let table = read_table(stream.borrow_mut(), table_location)?;

    //Check the number of sectors left after the table.
    // If the number of sectors left is less than or equal to the size of the table,
    // Then fill out the trail to the enc of the storage, then wrap around to the beginning

    stream.seek(SeekFrom::Start(table_location * 512))?;

    stream.write(&[0u8; 496])?;
    let mut current_trail: u128 = bincode::deserialize_from(& mut stream)?;

    let (new_location, last_trail) = if table_location + table._table_size >= METADATA_SPACE_SIZE - 1 {

        for i in table_location+1..METADATA_SPACE_SIZE {

            current_trail += 1;

            stream.seek(SeekFrom::Start( i * 512))?;

            stream.write(&[0u8; 496])?;

            bincode::serialize_into(& mut stream, &(current_trail as u128))?;
        }

        (0, current_trail+1)

    } else {
        (table_location+1, current_trail+1)
    };

    //Write the table to the next position
    write_table(stream.borrow_mut(), new_location, &table)?;

    //Leave behind a trail in the previous sector (make sure the rest of the sector is cleared too
    stream.seek(SeekFrom::Start(new_location * 512 + 496))?;
    bincode::serialize_into(& mut stream, &last_trail)?;

    //Update the TABLE_LOCATION variable
    set_table_location(new_location);

    Ok(())
}

///Called whenever the table is read or written to. Decrements the _accesses_left field and may initiate a shift
fn access<S: Read + Write + Seek>(mut stream: S) -> Result<()> {

    //Load the table
    let mut table = read_table(stream.borrow_mut(), get_table_location())?;

    //Decrement accesses left
    table._accesses_left -= 1;

    //Shift the table forward & reset the count
    if table._accesses_left == 0 {
        shift_table(stream.borrow_mut())?;

        table._accesses_left = table._accesses_per_shift;
    }

    //Write the new table
    write_table(stream.borrow_mut(), get_table_location(), &table)?;

    Ok(())
}

fn main() {
    let media_path = "test";

    create(media_path.borrow(), 512 * 1000).unwrap();

    let mut fp = OpenOptions::new()
        .read(true)
        .write(true)
        .open(media_path.borrow()).unwrap();


    format(fp.borrow_mut(), METADATA_SPACE_SIZE).unwrap();

    initialise(fp.borrow_mut()).unwrap();

    for _ in 0..500*9 {
        access(fp.borrow_mut()).unwrap();
    }

    println!("Search: {}", get_table_location());


}
