fn main() {
    let out_dir = format!("{}/protos", std::env::var("OUT_DIR").unwrap());

    std::fs::create_dir_all(&out_dir).unwrap();

    // Re-run if any proto changes
    println!("cargo:rerun-if-changed=protos/control.proto");
    println!("cargo:rerun-if-changed=protos/message.proto");
    println!("cargo:rerun-if-changed=protos/rendezvous.proto");

    protobuf_codegen::Codegen::new()
        .pure()
        .out_dir(out_dir)
        .inputs([
            "protos/rendezvous.proto",
            "protos/message.proto",
            "protos/control.proto",
        ])
        .include("protos")
        .customize(protobuf_codegen::Customize::default().tokio_bytes(true))
        .run()
        .expect("Codegen failed.");
}
