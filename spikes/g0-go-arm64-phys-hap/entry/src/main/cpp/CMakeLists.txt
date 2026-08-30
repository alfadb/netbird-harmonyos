cmake_minimum_required(VERSION 3.5.0)
project(g0_goloader LANGUAGES C)

add_library(goloader SHARED goloader.c)
target_compile_options(goloader PRIVATE
  -Wall
  -Wextra
  -Werror
  -Wno-error=unused-command-line-argument
)
target_link_libraries(goloader PUBLIC libace_napi.z.so)
