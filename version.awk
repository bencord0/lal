#!/usr/bin/awk -f

# Version strings are surrounded in double quotes
# Set the field separator to the " character
BEGIN {
    FS = "\""
}

# Find the first version in the file,
# this is (probably) the crate's own version.
/^version =/ {
    if (version == "") {
        version = $2
    }
}

# If no version was found, break the caller script
END {
    print version
    if (version == "") {
        exit 1
    }
}
