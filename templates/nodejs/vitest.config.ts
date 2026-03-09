import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Test environment
    environment: "node",
    
    // Global test timeout
    testTimeout: 10000,
    hookTimeout: 10000,
    
    // Coverage configuration
    coverage: {
      provider: "v8",
      reporter: ["text", "json", "html", "lcov"],
      exclude: [
        "node_modules/",
        "dist/",
        "tests/",
        "**/*.d.ts",
        "**/*.config.*",
        "**/coverage/**",
      ],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 80,
        statements: 80,
      },
    },
    
    // Test file patterns
    include: ["tests/**/*.test.ts", "src/**/*.test.ts"],
    exclude: ["node_modules/", "dist/"],
    
    // Reporters
    reporters: ["verbose"],
    
    // Enable type checking in tests
    typecheck: {
      enabled: true,
      checker: "tsc",
    },
  },
  
  // Resolve configuration
  resolve: {
    alias: {
      "@": "/src",
      "@tests": "/tests",
    },
  },
});
