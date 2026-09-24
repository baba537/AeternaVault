#!/bin/sh
# Runs the tests; on failure, the failing tests and their messages become one
# annotation, which is visible on the run page without signing in.
set -u
cargo test --locked > test-output.txt 2>&1
status=$?
cat test-output.txt
if [ $status -ne 0 ]; then
    summary=$(grep -E "^---- |panicked at|assertion|left:|right:|^error|FAILED|failed with" test-output.txt | head -n 60 | sed -e 's/%/%25/g' | awk '{printf "%s%%0A", $0}')
    echo "::error title=Tests failed::${summary}"
fi
exit $status
