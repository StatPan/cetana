use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "cuda")]
fn find_cuda_path() -> String {
    // Linux
    if let Ok(output) = Command::new("which").arg("nvcc").output() {
        if let Ok(path) = String::from_utf8(output.stdout) {
            if let Some(cuda_path) = path.trim().strip_suffix("/bin/nvcc") {
                return cuda_path.to_string();
            }
        }
    }

    // Windows
    for path in &[
        "C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA",
        "C:/CUDA",
    ] {
        if PathBuf::from(path).exists() {
            return path.to_string();
        }
    }

    "/usr/local/cuda".to_string()
}

#[cfg(feature = "vulkan")]
fn compile_vulkan_shaders() -> std::io::Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Compile reduction shader
    println!("cargo:rerun-if-changed=shaders/vulkan/reduction.comp");
    let status = Command::new("glslc")
        .args([
            "shaders/vulkan/reduction.comp",
            "-o",
            out_dir.join("reduction.spv").to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute glslc");

    if !status.success() {
        panic!("Failed to compile reduction shader");
    }

    // Compile binary operations shader
    println!("cargo:rerun-if-changed=shaders/vulkan/binary_ops.comp");
    let status = Command::new("glslc")
        .args([
            "shaders/vulkan/binary_ops.comp",
            "-o",
            out_dir.join("binary_ops.spv").to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute glslc");

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to compile binary operations shader",
        ))
    }
}

#[cfg(all(feature = "mps", target_os = "macos"))]
fn compile_metal_shaders() -> std::io::Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let shader_dir = PathBuf::from("shaders/metal");

    if !shader_dir.exists() {
        return Ok(()); // Skip if metal shaders directory doesn't exist
    }

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&out_dir)?;

    let shader_files = ["binary_ops.metal", "operations.metal", "reduction.metal"];

    for shader in shader_files.iter() {
        let shader_path = shader_dir.join(shader);
        if !shader_path.exists() {
            continue; // Skip if shader file doesn't exist
        }

        println!("cargo:rerun-if-changed=shaders/metal/{}", shader);

        // Compile .metal to .air
        let status = Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-c",
                shader_path.to_str().unwrap(),
                "-o",
                out_dir
                    .join(format!("{}.air", shader.replace(".metal", "")))
                    .to_str()
                    .unwrap(),
            ])
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to compile {}", shader),
            ));
        }
    }

    // Link .air files into metallib
    let air_files: Vec<String> = shader_files
        .iter()
        .map(|f| {
            out_dir
                .join(format!("{}.air", f.replace(".metal", "")))
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect();

    let status = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metallib",
            "-o",
            out_dir.join("shaders.metallib").to_str().unwrap(),
        ])
        .args(&air_files)
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to create metallib",
        ));
    }

    Ok(())
}

fn main() {
    #[cfg(feature = "cuda")]
    {
        println!("cargo:rerun-if-changed=cuda/");
        println!("cargo:rerun-if-changed=cuda-headers/");
        println!("cargo:rerun-if-changed=CMakeLists.txt");

        let cuda_path = find_cuda_path();
        let clangd_path = PathBuf::from(".clangd");

        if !clangd_path.exists() {
            let clangd_content = format!(
                r#"CompileFlags:
                Remove: 
                - "-forward-unknown-to-host-compiler"
                - "-rdc=*"
                - "-Xcompiler*"
                - "--options-file"
                - "--generate-code*"
                Add: 
                - "-xcuda"
                - "-std=c++14"
                - "-I{}/include"
                - "-I../../cuda-headers"
                - "--cuda-gpu-arch=sm_75"
                Compiler: clang

                Index:
                Background: Build

                Diagnostics:
                UnusedIncludes: None"#,
                cuda_path
            );

            fs::write(".clangd", clangd_content).expect("Failed to write .clangd file");
        }
        let dst = cmake::Config::new(".")
            .define("CMAKE_BUILD_TYPE", "Release")
            .define("CUDA_PATH", cuda_path.clone())
            .no_build_target(true)
            .build();

        // Search paths - include both lib and lib64
        println!("cargo:rustc-link-search={}/build/lib", dst.display());
        println!("cargo:rustc-link-search={}/build", dst.display());
        println!("cargo:rustc-link-search=native={}/lib", dst.display());
        println!("cargo:rustc-link-search=native={}/lib64", cuda_path.clone());
        println!("cargo:rustc-link-search=native={}/lib", cuda_path.clone());

        // CUDA runtime linking - only essential libraries
        println!("cargo:rustc-link-lib=cudart");
        println!("cargo:rustc-link-lib=cuda");

        // Static libraries - if they exist
        if PathBuf::from(format!("{}/build/lib/libnn_ops.a", dst.display())).exists() {
            println!("cargo:rustc-link-arg=-Wl,--whole-archive");
            println!("cargo:rustc-link-lib=static=nn_ops");
            println!("cargo:rustc-link-lib=static=tensor_ops");
            println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
        }
    }

    // Compile Vulkan shaders only if the "vulkan" feature is enabled
    #[cfg(feature = "vulkan")]
    {
        println!("cargo:rerun-if-changed=shaders/vulkan/");
        compile_vulkan_shaders().expect("Failed to compile Vulkan shaders");
    }

    // Compile Metal shaders only if the "metal" feature is enabled and on macOS
    #[cfg(all(feature = "mps", target_os = "macos"))]
    {
        println!("cargo:rerun-if-changed=shaders/metal/");
        compile_metal_shaders().expect("Failed to compile Metal shaders");
    }
}
