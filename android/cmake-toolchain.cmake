# Toolchain per whisper.cpp (cmake-rs non passa ANDROID_ABI da solo): arm64, API 26, libc++ condivisa come il resto dell'APK.
set(ANDROID_ABI arm64-v8a)
set(ANDROID_PLATFORM android-26)
set(ANDROID_STL c++_shared)
include("$ENV{ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake")
