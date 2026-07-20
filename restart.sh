#!/bin/bash
toolforge webservice buildservice restart  --health-check-path '/healthz' --mount=none --mem 2G --cpu 2
