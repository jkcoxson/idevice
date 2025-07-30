# Jackson Coxson

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
cmake_minimum_required(VERSION 3.10)
project(IdeviceFFI C)

# Set the paths
set(HEADER_FILE ${CMAKE_SOURCE_DIR}/../idevice.h)
set(STATIC_LIB ${CMAKE_SOURCE_DIR}/../../target/release/libidevice_ffi.a)
set(EXAMPLES_DIR ${CMAKE_SOURCE_DIR}/../examples)

# Find all C example files
file(GLOB EXAMPLE_SOURCES ${EXAMPLES_DIR}/*.c)

find_package(PkgConfig REQUIRED)

# Create an executable for each example file
foreach(EXAMPLE_FILE ${EXAMPLE_SOURCES})
    # Extract the filename without the path
    get_filename_component(EXAMPLE_NAME ${EXAMPLE_FILE} NAME_WE)

    # Create an executable for this example
    add_executable(${EXAMPLE_NAME} ${EXAMPLE_FILE})

    # Include the generated header
    target_include_directories(${EXAMPLE_NAME} PRIVATE ${CMAKE_SOURCE_DIR}/..)

    # Link the static Rust library
    target_link_libraries(${EXAMPLE_NAME} PRIVATE ${STATIC_LIB})

    if(UNIX AND NOT APPLE)
      target_link_libraries(${EXAMPLE_NAME} PRIVATE m)
    endif()

    # Bulk-link common macOS system frameworks
    if(APPLE)
      target_link_libraries(${EXAMPLE_NAME} PRIVATE
          "-framework CoreFoundation"
          "-framework Security"
          "-framework SystemConfiguration"
          "-framework CoreServices"
          "-framework IOKit"
          "-framework CFNetwork"
      )
    endif()
endforeach()

