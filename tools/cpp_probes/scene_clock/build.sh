#!/bin/sh
# Builds every probe twice: "_app" against the app binary's objects (ClockSceneNative Release: -O3 -ffp-contract=off,
# SCENE_NATIVE_ONLY) and "_strict" against the tests' strict objects (ClockScene Release: -O3 -ffp-model=strict -ffp-contract=off).
set -e
cd "$(dirname "$0")"
R=/home/jackh/Documents/git/CrystalOSD
B=$R/build/linux/CMakeFiles
for p in scenario cold text cubes extra; do
  clang++ -std=c++26 -O3 -DNDEBUG -ffp-contract=off -DSCENE_NATIVE_ONLY -I$R/Source ${p}_probe.cpp $R/Source/App/Golden.cpp $B/ClockSceneNative.dir/Release/Source/Scene/*.o -o ${p}_app
  clang++ -std=c++26 -O3 -DNDEBUG -ffp-contract=off -ffp-model=strict -I$R/Source ${p}_probe.cpp $R/Source/App/Golden.cpp $B/ClockScene.dir/Release/Source/Scene/*.o -o ${p}_strict
done
clang++ -std=c++26 -O3 -DNDEBUG -ffp-contract=off -DSCENE_NATIVE_ONLY -I$R/Source golden_probe.cpp $R/Source/App/Golden.cpp $B/ClockSceneNative.dir/Release/Source/Scene/*.o $B/OpeningSceneNative.dir/Release/Source/Scene/opening/*.o -o golden_probe
