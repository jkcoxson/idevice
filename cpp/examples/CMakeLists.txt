# Jackson Coxson

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
cmake_minimum_required(VERSION 3.10)
project(IdeviceFFI CXX)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Set the paths
set(HEADER_FILE ${CMAKE_SOURCE_DIR}/../../ffi/idevice.h)
set(STATIC_LIB ${CMAKE_SOURCE_DIR}/../../target/release/libidevice_ffi.a)
set(EXAMPLES_DIR ${CMAKE_SOURCE_DIR}/../examples)

set(IDEVICE_CPP_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../include)  # cpp/include
set(IDEVICE_FFI_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../../ffi)    # ffi/

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -Wpedantic")

find_package(PkgConfig REQUIRED)

# Find all C++ example files
file(GLOB EXAMPLE_SOURCES ${EXAMPLES_DIR}/*.cpp)

# Create an executable for each example file
foreach(EXAMPLE_FILE ${EXAMPLE_SOURCES})
    get_filename_component(EXAMPLE_NAME ${EXAMPLE_FILE} NAME_WE)
    add_executable(${EXAMPLE_NAME} ${EXAMPLE_FILE})

    target_include_directories(${EXAMPLE_NAME} PRIVATE
        ${IDEVICE_CPP_INCLUDE_DIR}
        ${IDEVICE_FFI_INCLUDE_DIR}
    )

    # Include the generated header
    target_include_directories(${EXAMPLE_NAME} PRIVATE ${CMAKE_SOURCE_DIR}/..)

    # Link the static Rust library
    target_link_libraries(${EXAMPLE_NAME} PRIVATE ${STATIC_LIB})


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


