cmake_minimum_required(VERSION 3.5.0)
project(e3_fdprobe LANGUAGES C)

add_library(fdprobe SHARED fd_probe.c)
target_compile_options(fdprobe PRIVATE
  -Wall
  -Wextra
  -Werror
  -Wno-error=unused-command-line-argument
)
target_link_libraries(fdprobe PUBLIC libace_napi.z.so)
