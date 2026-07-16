cmake_minimum_required(VERSION 3.5.0)
project(r1_api24_probe LANGUAGES CXX)

add_library(probe SHARED probe.cpp)
target_compile_features(probe PRIVATE cxx_std_17)
target_link_libraries(probe PUBLIC libace_napi.z.so libhilog_ndk.z.so libdl.so)
