fn main() {
    tonic_build::compile_protos("src/proto/hello.proto").unwrap();
}
