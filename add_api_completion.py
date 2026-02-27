import re

with open('src/api/mod.rs', 'r') as f:
    content = f.read()

completion_method = """
    /// Send a completion request (e.g. for FIM)
    pub async fn completion(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
        stop: Option<Vec<String>>,
    ) -> Result<types::CompletionResponse> {
        let url = format!("{}/completions", self.base_url);

        let req = types::CompletionRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            max_tokens,
            temperature: Some(0.1),
            top_p: Some(0.9),
            stop,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::ServerError {
                status: status.as_u16(),
                message: text,
            }
            .into());
        }

        let resp: types::CompletionResponse = response.json().await?;
        Ok(resp)
    }
"""

# Insert before `pub async fn chat(`
content = content.replace('    pub async fn chat(', completion_method + '\n    pub async fn chat(')

with open('src/api/mod.rs', 'w') as f:
    f.write(content)
