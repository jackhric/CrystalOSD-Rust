#!/bin/sh
# Builds the probes against the C++ Release build tree (never writes inside the C++ repository).
#   GoldenRecorder: the real CrystalOSD app objects, NativeRenderer calls recorded by Wrap.cpp (-Wl,--wrap).
#   Replay: a stream through the real NativeRenderer on a headless Device.
#   Synthetic: writes the synthetic stream.
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
CPP=/home/jackh/Documents/git/CrystalOSD
B=$CPP/build/linux
D=$CPP/build/_deps
OUT=$HERE/bin
mkdir -p "$OUT"
FLAGS="-O3 -DNDEBUG -std=c++26 -ffp-contract=off -D_MSVC_STL_HARDENING=1"
INC="-I$CPP/Source -I$D/vma-src/include -I$CPP/ThirdParty -I$D/vk-bootstrap-src/src -I$B/_deps/sdl3-build/include-revision -I$D/SDL3-src/include -I$D/json-src/include -I$HERE"
R=$B/Release
APPLIBS="$R/libClockRender.a $R/libClockSceneNative.a $R/libImGui.a $R/libClockAssets.a $R/libClockInput.a $R/libBootScreens.a $R/libHostInputs.a $R/libClockAudioLive.a $R/libClockVma.a $R/libClockStb.a $D/../linux/_deps/vk-bootstrap-build/Release/libvk-bootstrap.a -ldl /usr/lib/libvulkan.so $R/libClockScene.a $R/libOpeningSceneNative.a $R/libClockScreen.a $B/_deps/sdl3-build/Release/libSDL3.so.0.4.18 $R/libClockAudioFacade.a $R/libClockAudioSnapshot.a $R/libClockAudioData.a $R/libClockAudioDriver.a $R/libClockAudioSpu2.a $R/libClockAudioCore.a"
RPATH="-Wl,-rpath,$B/_deps/sdl3-build/Release"

clang++ $FLAGS $INC -c "$HERE/Wrap.cpp" -o "$OUT/Wrap.o"
WRAPS=""
for s in _ZN6render14NativeRenderer4drawERKN5scene5FrameE _ZN6render14NativeRenderer9configureERKNS_12NativeOutputE \
         _ZN6render14NativeRenderer10setTextureEijjSt4spanIKhLm18446744073709551615EE \
         _ZN6render14NativeRenderer10setTextureEN5scene10TextureSetEijjSt4spanIKhLm18446744073709551615EE \
         _ZN6render14NativeRenderer13setGlyphCacheERKN5scene4FontERKNS1_10GlyphCacheE \
         _ZN6render14NativeRenderer10readTargetEN5scene10TargetNameE; do
    WRAPS="$WRAPS -Wl,--wrap=$s"
done
O=$B/CMakeFiles/CrystalOSD.dir/Release/Source/App
clang++ -O3 -fuse-ld=lld $WRAPS $O/Main.cpp.o $O/Golden.cpp.o $O/GoldenScenario.cpp.o $O/DebugPanel.cpp.o $O/Png.cpp.o $O/Profile.cpp.o $O/ClockAssets.cpp.o \
    "$OUT/Wrap.o" -o "$OUT/GoldenRecorder" $RPATH $APPLIBS

clang++ $FLAGS $INC "$HERE/Replay.cpp" -o "$OUT/Replay" -fuse-ld=lld $RPATH $APPLIBS
clang++ $FLAGS $INC "$HERE/Synthetic.cpp" -o "$OUT/Synthetic" -fuse-ld=lld
echo built
