import { describe, it, expect } from "vitest";
import {
  reverseString,
  countVowels,
  isPalindrome,
  toSnakeCase,
  truncate,
} from "../src/stringOps.js";

describe("reverseString", () => {
  it("reverses a string", () => {
    expect(reverseString("hello")).toBe("olleh");
    expect(reverseString("")).toBe("");
    expect(reverseString("a")).toBe("a");
    expect(reverseString("12345")).toBe("54321");
  });
});

describe("countVowels", () => {
  it("counts vowels correctly", () => {
    expect(countVowels("hello")).toBe(2);
    expect(countVowels("AEIOU")).toBe(5);
    expect(countVowels("xyz")).toBe(0);
    expect(countVowels("")).toBe(0);
    expect(countVowels("Hello World")).toBe(3);
  });
});

describe("isPalindrome", () => {
  it("detects palindromes", () => {
    expect(isPalindrome("racecar")).toBe(true);
    expect(isPalindrome("A man a plan a canal Panama")).toBe(true);
    expect(isPalindrome("hello")).toBe(false);
    expect(isPalindrome("")).toBe(true);
    expect(isPalindrome("a")).toBe(true);
    expect(isPalindrome("Was it a car or a cat I saw")).toBe(true);
  });
});

describe("toSnakeCase", () => {
  it("converts to snake_case", () => {
    expect(toSnakeCase("camelCase")).toBe("camel_case");
    expect(toSnakeCase("PascalCase")).toBe("pascal_case");
    expect(toSnakeCase("simple")).toBe("simple");
    expect(toSnakeCase("XMLParser")).toBe("xml_parser");
    expect(toSnakeCase("getHTTPResponse")).toBe("get_http_response");
  });
});

describe("truncate", () => {
  it("truncates strings", () => {
    expect(truncate("hello world", 8)).toBe("hello...");
    expect(truncate("short", 10)).toBe("short");
    expect(truncate("hello world", 5, "..")).toBe("he..");
    expect(truncate("", 5)).toBe("");
    expect(truncate("exact", 5)).toBe("exact");
  });
});
