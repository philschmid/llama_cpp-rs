//! Test harness for [`llama_cpp`][lcpp] and [`llama_cpp_sys`].
//!
//! This crate expects one or more `llama.cpp`-compatible GGUF models to be made available in
//! a directory specified in the `LLAMA_CPP_TEST_MODELS` environment variable.

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::Write;
    use std::time::Duration;

    use tokio::select;
    use tokio::time::Instant;

    use llama_cpp::standard_sampler::StandardSampler;
    use llama_cpp::{LlamaModel, LlamaParams, SessionParams};

    async fn list_models() -> Vec<String> {
        let dir = std::env::var("LLAMA_CPP_TEST_MODELS").unwrap_or_else(|_| {
            eprintln!(
                "LLAMA_CPP_TEST_MODELS environment variable not set. \
                Please set this to the directory containing one or more GGUF models."
            );

            std::process::exit(1)
        });

        let dir = std::path::Path::new(&dir);

        if !dir.is_dir() {
            panic!("\"{}\" is not a directory", dir.to_string_lossy());
        }

        let mut models = tokio::fs::read_dir(dir).await.unwrap();
        let mut rv = vec![];

        while let Some(model) = models.next_entry().await.unwrap() {
            let path = model.path();

            if path.is_file() {
                let path = path.to_str().unwrap();
                if path.ends_with(".gguf") {
                    rv.push(path.to_string());
                }
            }
        }

        rv
    }

    // TODO theres a concurrency issue with vulkan, look into it
    #[ignore]
    #[tokio::test]
    async fn load_models() {
        let models = list_models().await;

        for model in models {
            println!("Loading model: {}", model);
            let _model = LlamaModel::load_from_file_async(model, LlamaParams::default())
                .await
                .expect("Failed to load model");
        }
    }

    #[tokio::test]
    async fn execute_completions() {
        let models = list_models().await;

        for model in models {
            let mut params = LlamaParams::default();

            if cfg!(any(feature = "vulkan", feature = "cuda", feature = "metal")) {
                params.n_gpu_layers = u32::MAX;
            }

            let model = LlamaModel::load_from_file_async(model, params)
                .await
                .expect("Failed to load model");

            let mut params = SessionParams::default();
            params.n_ctx = 2048;
            let mut session = model
                .create_session(params)
                .expect("Failed to create session");

            session
                .advance_context_async("<|SYSTEM|>You are a helpful assistant.")
                .await
                .unwrap();
            session
                .advance_context_async("<|USER|>How would you approach the trolley problem?")
                .await
                .unwrap();
            session
                .advance_context_async("<|ASSISTANT|>")
                .await
                .unwrap();

            let mut completions = session.start_completing_with(StandardSampler::default(), 1024);
            let timeout_by = Instant::now() + Duration::from_secs(500);

            println!();

            loop {
                select! {
                    _ = tokio::time::sleep_until(timeout_by) => {
                        break;
                    }
                    completion = completions.next_token_async() => {
                        if let Some(token) = completion {
                            if token == model.eos() {
                                break;
                            }

                            let formatted = model.token_to_piece(token);
                            print!("{formatted}");
                            let _ = io::stdout().flush();
                        }
                        continue;
                    }
                }
            }
            println!();
            println!();
        }
    }
}
