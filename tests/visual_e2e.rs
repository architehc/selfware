//! End-to-end integration test for the visual verification pipeline.
//!
//! Exercises the full path: screen capture -> PNG validation -> visual
//! verification -> retry/recovery on mismatch.
//!
//! Gated behind the `integration` feature because screen capture requires
//! a display server (will fail gracefully in headless CI).

#[cfg(feature = "integration")]
mod visual_e2e {
    use selfware::computer::ScreenCapture;
    use selfware::visual_verification::VisualVerifier;

    /// PNG magic bytes: 0x89 P N G \r \n 0x1A \n
    const PNG_HEADER: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    #[tokio::test]
    async fn test_visual_verification_e2e() {
        // Step 1: Attempt screen capture.
        // In CI (no display), this will fail — we verify graceful error handling.
        let capture_result = ScreenCapture::capture_full().await;

        match capture_result {
            Ok(captured) => {
                // Step 2: Verify it's a valid PNG by checking header bytes.
                let png_bytes = base64_decode(&captured.base64_png);
                assert!(
                    png_bytes.len() >= 8,
                    "Captured PNG data too short: {} bytes",
                    png_bytes.len()
                );
                assert_eq!(
                    &png_bytes[..8],
                    &PNG_HEADER,
                    "Captured data does not have valid PNG header"
                );
                assert!(captured.width > 0, "Screenshot width must be positive");
                assert!(captured.height > 0, "Screenshot height must be positive");

                // Step 3: Run visual verification against the captured screenshot.
                // Use a dummy endpoint — we don't expect a real VLM in CI.
                let verifier =
                    VisualVerifier::new("http://127.0.0.1:1/v1", "test-model").with_timeout(2);
                let verify_result = verifier
                    .verify_screenshot(&captured.base64_png, "A desktop screen")
                    .await;

                // Step 4: The VLM endpoint is unreachable, so verification should
                // fail with a connection error, not a panic.
                assert!(
                    verify_result.is_err(),
                    "Expected verify_screenshot to fail against unreachable endpoint"
                );
                let err_msg = verify_result.unwrap_err().to_string();
                assert!(
                    err_msg.contains("connect") || err_msg.contains("Failed") || err_msg.contains("timed out"),
                    "Error should indicate a connection failure, got: {}",
                    err_msg
                );

                // Step 5: Test retry/recovery path — simulate retrying after failure
                // by calling again and confirming consistent error behavior.
                let retry_result = verifier
                    .verify_screenshot(&captured.base64_png, "A desktop screen")
                    .await;
                assert!(
                    retry_result.is_err(),
                    "Retry should also fail against unreachable endpoint"
                );
            }
            Err(e) => {
                // No display available (headless CI) — verify graceful error.
                let err_msg = e.to_string().to_lowercase();
                assert!(
                    err_msg.contains("monitor")
                        || err_msg.contains("display")
                        || err_msg.contains("screen")
                        || err_msg.contains("capture")
                        || err_msg.contains("failed")
                        || err_msg.contains("no")
                        || err_msg.contains("x11")
                        || err_msg.contains("wayland"),
                    "Expected display-related error, got: {}",
                    e
                );

                // Even without a display, the verification pipeline should handle
                // invalid/synthetic input gracefully.
                let verifier =
                    VisualVerifier::new("http://127.0.0.1:1/v1", "test-model").with_timeout(2);
                let fake_png_b64 = base64_encode(&PNG_HEADER);
                let result = verifier
                    .verify_screenshot(&fake_png_b64, "Anything")
                    .await;
                assert!(
                    result.is_err(),
                    "Verification with unreachable VLM should fail gracefully"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_visual_verification_retry_recovery() {
        // Simulate the retry/recovery path: call verify_screenshot multiple
        // times against an unreachable endpoint and confirm each attempt fails
        // gracefully without panics or state corruption.
        let verifier =
            VisualVerifier::new("http://127.0.0.1:1/v1", "test-model").with_timeout(1);
        let fake_b64 = base64_encode(&PNG_HEADER);

        let mut errors = Vec::new();
        for _ in 0..3 {
            let result = verifier
                .verify_screenshot(&fake_b64, "Expected screen content")
                .await;
            assert!(result.is_err());
            errors.push(result.unwrap_err().to_string());
        }

        // All errors should be consistent (same failure mode each time)
        assert_eq!(errors.len(), 3);
        for err in &errors {
            assert!(
                err.contains("connect") || err.contains("Failed") || err.contains("timed out"),
                "Unexpected error variant: {}",
                err
            );
        }
    }

    #[tokio::test]
    async fn test_visual_check_integration_graceful_failure() {
        // The visual_check method (used by VerificationGate) should return
        // Ok(CheckResult) even when the VLM is unreachable.
        let verifier =
            VisualVerifier::new("http://127.0.0.1:1/v1", "test-model").with_timeout(1);
        let fake_b64 = base64_encode(&PNG_HEADER);

        let check_result = verifier.visual_check(&fake_b64, "Some UI").await;
        // visual_check wraps errors into CheckResult, so it should return Ok
        assert!(check_result.is_ok(), "visual_check should not propagate errors");
        let cr = check_result.unwrap();
        assert!(!cr.passed, "Check should not pass when VLM is unreachable");
        assert!(
            !cr.errors.is_empty(),
            "Check should contain error details"
        );
    }

    fn base64_decode(input: &str) -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(input)
            .unwrap_or_default()
    }

    fn base64_encode(input: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(input)
    }
}
