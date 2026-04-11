fn main() {
    for f in [
        "miniz.c",
        "miniz.h",
        "miniz_common.h",
        "miniz_export.h",
        "miniz_tdef.c",
        "miniz_tdef.h",
        "miniz_tinfl.c",
        "miniz_tinfl.h",
        "miniz_zip.c",
        "miniz_zip.h",
        "wrapper.c",
        "wrapper.h",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }

    cc::Build::new()
        .file("miniz.c")
        .file("miniz_tdef.c")
        .file("miniz_tinfl.c")
        .file("miniz_zip.c")
        .file("wrapper.c")
        .include(".")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-deprecated-non-prototype")
        .compile("miniz_wrapper");
}
