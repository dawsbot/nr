#!/bin/sh
echo "argc=$#"
for a in "$@"; do
    echo "arg:$a"
done
