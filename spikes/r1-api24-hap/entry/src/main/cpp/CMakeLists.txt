cmake_minimum_required(VERSION 3.5.0)
project(r1_api24_probe LANGUAGES CXX)

add_library(probe SHARED e1_c_probe.cpp)
target_compile_features(probe PRIVATE cxx_std_17)
target_compile_options(probe PRIVATE -pthread)
target_link_options(probe PRIVATE -pthread)
target_link_libraries(probe PUBLIC libace_napi.z.so libhilog_ndk.z.so)

add_library(e2network SHARED e2_c_network.cpp)
target_compile_features(e2network PRIVATE cxx_std_17)
target_compile_options(e2network PRIVATE -Wall -Wextra -Werror -Wno-error=unused-command-line-argument)
target_link_libraries(e2network PUBLIC libace_napi.z.so libhilog_ndk.z.so)
