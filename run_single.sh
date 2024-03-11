#!/bin/bash
toolforge jobs delete single

toolforge jobs run --wait --mem 2000Mi --cpu 1 --mount=all --image tool-ac2wd/tool-ac2wd:latest --command "sh -c 'target/release/auth2wd $1 $2'" single

toolforge jobs logs single
