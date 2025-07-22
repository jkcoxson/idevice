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

    # libplist

    if( APPLE )
      # use static linking
      find_library( LIBPLIST libplist-2.0.a REQUIRED )
      message( STATUS "(Static linking) LIBPLIST " ${LIBPLIST} )
      target_link_libraries ( ${EXAMPLE_NAME} PRIVATE ${LIBPLIST} )
    elseif( WIN32)
    pkg_search_module(PLIST REQUIRED libplist-2.0)
      find_library( LIBPLIST ${PLIST_LIBRARIES} PATH ${PLIST_LIBDIR} )
      target_link_libraries ( ${EXAMPLE_NAME} PRIVATE ${LIBPLIST} )
    else ()
      pkg_search_module(PLIST libplist>=2.0)
      if(NOT PLIST_FOUND)
        pkg_search_module(PLIST REQUIRED libplist-2.0)
      endif()
      find_library( LIBPLIST ${PLIST_LIBRARIES} PATH ${PLIST_LIBDIR} )
      target_link_libraries ( ${EXAMPLE_NAME} PUBLIC ${LIBPLIST} )
    endif()
    if ( PLIST_FOUND )
      message( STATUS "found libplist-${PLIST_VERSION}" )
    endif()
    target_include_directories( ${EXAMPLE_NAME} PRIVATE ${PLIST_INCLUDE_DIRS} )

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


