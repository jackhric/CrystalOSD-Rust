#!/bin/bash
# Builds the opening probe from the C++ at /home/jackh/Documents/git/CrystalOSD (commit 039cc58): opening_probe (-O2, the reference flags) and opening_probe_O0.
set -e
cd "$(dirname "$0")"
S=/home/jackh/Documents/git/CrystalOSD/Source
SRC=(opening_probe.cpp $S/Scene/Arithmetic.cpp $S/Scene/ProgramImage.cpp $S/Scene/ColdState.cpp $S/Scene/ClockState.cpp $S/Scene/opening/*.cpp)
clang++ -std=c++2c -O2 -ffp-contract=off -ffp-model=strict -I$S -I. "${SRC[@]}" -o opening_probe
clang++ -std=c++2c -O0 -ffp-contract=off -ffp-model=strict -I$S -I. "${SRC[@]}" -o opening_probe_O0
