cmake_minimum_required(VERSION 3.21)
project(nl_ffi LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
find_package(Qt6 REQUIRED COMPONENTS Widgets)
qt_standard_project_setup()

# Corrosion builds the Rust crate for the right target and adds the platform's system libraries to the link line.
include(FetchContent)
FetchContent_Declare(Corrosion GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git GIT_TAG v0.6.1)
FetchContent_MakeAvailable(Corrosion)
corrosion_import_crate(MANIFEST_PATH rust/Cargo.toml)

qt_add_executable(nl_ffi MACOSX_BUNDLE app/main.cpp)
target_include_directories(nl_ffi PRIVATE app)
target_link_libraries(nl_ffi PRIVATE Qt6::Widgets skr_core)
