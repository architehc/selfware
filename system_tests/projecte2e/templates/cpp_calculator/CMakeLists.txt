cmake_minimum_required(VERSION 3.14)
project(calculator)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Main library
add_library(calculator src/calculator.cpp)
target_include_directories(calculator PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)

# Enable testing
enable_testing()

# Fetch GoogleTest
include(FetchContent)
FetchContent_Declare(
  googletest
  URL https://github.com/google/googletest/archive/refs/tags/release-1.12.1.zip
)
FetchContent_MakeAvailable(googletest)

# Test executable
add_executable(calculator_test tests/calculator_test.cpp)
target_link_libraries(calculator_test calculator gtest_main)

# Add tests
include(GoogleTest)
gtest_discover_tests(calculator_test)
