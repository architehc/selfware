use selfware::api::{ApiClient, Message, ThinkingMode};
use selfware::config::Config;
use anyhow::Result;

#[tokio::test]
#[ignore] // Run manually with: cargo test --test system_test -- --ignored
async fn test_living_endpoint() -> Result<()> {
    let mut config = Config::default();
    config.endpoint = "https://crazyshit.ngrok.io/v1".to_string();
    config.model = "Qwen/Qwen3-Coder-Next-FP8".to_string();
    
    let client = ApiClient::new(config);
    
    let messages = vec![
        Message::user("Return exactly the word 'OK' and nothing else.")
    ];
    
    println!("Sending request to {}...", client.base_url());
    
    let response = client.chat(messages, None, ThinkingMode::Disabled).await?;
    
    println!("Response received: {}", response.choices[0].message.content);
    println!("Usage: prompt={}, completion={}", 
        response.usage.prompt_tokens, 
        response.usage.completion_tokens);
    
    assert!(response.choices[0].message.content.contains("OK"));
    assert!(response.usage.total_tokens > 0);
    
    Ok(())
}
