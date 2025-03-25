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
find_package(OpenSSL REQUIRED)

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

    # Link OpenSSL
    target_link_libraries(${EXAMPLE_NAME} PRIVATE OpenSSL::SSL OpenSSL::Crypto)

    # libplist

    if( APPLE )
      # use static linking
      pkg_search_module(PLIST REQUIRED libplist-2.0)
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

