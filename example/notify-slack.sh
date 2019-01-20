#!/bin/bash

echo "Should notify slack channel that container with id $1 has failed!";
echo "stderr1" 1>&2;
sleep 5;
echo "Even long operations (emulated by sleep) shouldn't block the checker"
echo "stderr2" 1>&2;
exit 0;