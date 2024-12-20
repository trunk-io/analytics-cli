fn main() {
    let file_descriptors = protox::compile(["proto/test_context.proto"], ["proto/"]).unwrap();
    prost_build::compile_fds(file_descriptors).unwrap();
}
