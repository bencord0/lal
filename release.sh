#!/bin/bash
set -ex
version="$(./version.awk < Cargo.toml)"
git tag -a "v${versioon}" -m "${version}"
