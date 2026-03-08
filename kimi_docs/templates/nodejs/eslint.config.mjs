import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import security from "eslint-plugin-security";
import globals from "globals";

export default tseslint.config(
  // Base ESLint recommended rules
  eslint.configs.recommended,
  
  // TypeScript strict rules
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  
  // Security rules
  security.configs.recommended,
  
  // Custom configuration
  {
    languageOptions: {
      parserOptions: {
        project: "./tsconfig.json",
        tsconfigRootDir: import.meta.dirname,
      },
      globals: {
        ...globals.node,
        ...globals.es2022,
      },
    },
    rules: {
      // TypeScript-specific rules
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/explicit-function-return-type": "warn",
      "@typescript-eslint/explicit-module-boundary-types": "warn",
      "@typescript-eslint/no-unused-vars": ["error", { "argsIgnorePattern": "^_" }],
      "@typescript-eslint/prefer-nullish-coalescing": "error",
      "@typescript-eslint/prefer-optional-chain": "error",
      "@typescript-eslint/strict-boolean-expressions": "warn",
      
      // Security rules
      "security/detect-object-injection": "error",
      "security/detect-non-literal-regexp": "warn",
      "security/detect-unsafe-regex": "error",
      "security/detect-buffer-noassert": "error",
      "security/detect-eval-with-expression": "error",
      "security/detect-no-csrf-before-method-override": "error",
      "security/detect-non-literal-fs-filename": "warn",
      "security/detect-non-literal-require": "warn",
      "security/detect-possible-timing-attacks": "warn",
      "security/detect-pseudoRandomBytes": "error",
      
      // General best practices
      "no-console": "warn",
      "no-debugger": "error",
      "eqeqeq": ["error", "always"],
      "curly": ["error", "all"],
      "no-throw-literal": "error",
      "prefer-const": "error",
      "no-var": "error",
    },
  },
  
  // Test file overrides
  {
    files: ["**/*.test.ts", "**/*.spec.ts", "tests/**/*"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/explicit-function-return-type": "off",
      "security/detect-object-injection": "off",
      "no-console": "off",
    },
  },
  
  // Ignore patterns
  {
    ignores: [
      "dist/",
      "node_modules/",
      "coverage/",
      "*.config.js",
      "*.config.mjs",
    ],
  }
);
