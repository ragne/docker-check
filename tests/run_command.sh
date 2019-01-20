#!/bin/bash

echo "Should have a container id: $1";
echo "should capture stderr as well" 1>&2;
if [[ "$1" == "some-id-error" ]]; then 
  echo "Finished with error" 1>&2;
  exit 1;
else
  echo "finished successfully"
  exit 0;
fi