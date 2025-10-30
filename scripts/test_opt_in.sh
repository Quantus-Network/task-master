./sc    #!/bin/bash

# Run opt_in repository tests with single-threaded execution
# This ensures tests don't interfere with each other's database state

cargo test --lib opt_in -- --test-threads=1

