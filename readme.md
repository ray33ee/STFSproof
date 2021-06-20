# STFSproof

STFSproof is a proof of concept implementation of STFS (Shifting table filesystem) which is designed to minimise the write intensity for all sectors to increase the longevity of storage media.

## Why?

Most filesystems, even the most basic, must keep track of basic metadata (at the very least the length of a file). This means that frequent access to a file will result in frequent access to the sector(s) that contain metadata. For applications that are write intensive or that run for a long time, (like data logging) this can result in those metadata sectors failing and at best corrupting the file, and at worst corrupting system files.

STFS solves this issue by occasionally moving its table around to spread the reads and writes over a large enough pool of sectors. This does mean extra total writes (since extra writes must be used to move the table) but since the table is moved within a large space (known as metadata space) this is not an issue.

## Tracking the table

The first issue is that if the table is shifting, how do we keep track of it? The obvious answer would be to reserve a few bytes at the beginning of the filesystem to record the location of the table. This is not suitable for two reasons

1. If the media is removed or interrupted before it can write the location, then the location is lost or corrupt
2. The write location itself becomes a read/write hotspot

The approach we propose (and is used in STFSproof) is a two pronged attack

1. When the filesystem is mounted for the first time, we manually search for the table
2. After the table is found, we store the location in memory for fast access

To ensure that the search is fast, whenever we shift the table we leave behind a trail (and when we format we create an initial trail), which enables us to use a binary split-style algorithm. Each sector in metadata space reserves 16 bytes at the end to store a part of the trail. Whenever we shift, we shift forward by one sector, and in those top 16 bytes of the first sector we move into, we enter a number, which is simply incremented every shift. When we get near the end, we wrap around to the beginning and keep incrementing.

When we search for the table, we look for consecutive numbers that decrease, instead of increase. This discontinuity represents the start of the table. To demonstrate, we use a metadata space of 5 sectors. When the device is formatted, the table is at location zero (indicated by T) and we have the following sequence (the nth number in the sequence represents the nth number in the trail)

T
5 1 2 3 4

If we need to search for the table at this stage, we can split the sequence in half, the left half [5, 1, 2] decreases (goes from 5 to 2) so contains the table. Recursively splitting gives the location of the table.

If we write to the table enough times, we will initiate a shift. We move the table from index 0 to 1, and we take the trail number so far (5) and increment it, and input it in the first sector of the table. This gives the sequence

T
5 6 2 3 4

As before, we can use a binary search algorithm focusing on ranges that decrease. After a few shifts, we get the sequence

      T
5 6 7 8 4

The next shift might give

        T
5 6 7 8 9

This however does not contain a decrease, so we cannot identify the table. To avoid this, we never move the table to the last position, instead we wrap to the beginning, giving the sequence

T
10 6 7 8 9

Which gives our decrease. This keeps going on, these numbers continuously increasing. Since the trail numbers are stored as 128-bit numbers, we can keep incremementing without fear of overflow.

# Table

The table itself keeps track of metadata you would associate with a filesystem table, but it also contains extra fields specifically for STFS:

- `_accesses_left`: The number of reads/writes to the filesystem before a shift is initiated
- `_accesses_per_shift`: The maximum number of reads/writes allowed before a shift is initiated (this value can be changed at any time)

This allows us to strike a balance between spreading the load over multiple sectors (by keeping `_accesses_per_shift`) as low as possible, and maximsing performance (by keeping `_accesses_per_shift` as high as possible). Modifying this value at any time allows users to fine tune performance

# Limitations of STFS

## Metadata space
Since the table is shifted over the metadata space, this space must be reserved and is not available to the user. Since the table size is very small, a space of a few thousand sectors should be enough, and will not represent a large proportion of modern storage media.

# Performance
Some reads and writes will take longer than expected, if they initiate a shift

# Limitations of STFSproof

STFSproof is, as already mentioned, a proof of concept. It makes no attempt at optimised filesystem code. Also to keep the table as simple as possible, only a single file (no directories) is allowed in the filesystem. With that said, it is possible to create a more sophisticated filesystem, we simply need a more sophisticated table.
