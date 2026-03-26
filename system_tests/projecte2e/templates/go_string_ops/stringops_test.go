package stringops

import (
	"testing"
)

func TestReverseString(t *testing.T) {
	tests := []struct {
		input, want string
	}{
		{"hello", "olleh"},
		{"", ""},
		{"a", "a"},
		{"12345", "54321"},
	}

	for _, tt := range tests {
		got := ReverseString(tt.input)
		if got != tt.want {
			t.Errorf("ReverseString(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

func TestCountVowels(t *testing.T) {
	tests := []struct {
		input string
		want  int
	}{
		{"hello", 2},
		{"AEIOU", 5},
		{"xyz", 0},
		{"", 0},
		{"Hello World", 3},
	}

	for _, tt := range tests {
		got := CountVowels(tt.input)
		if got != tt.want {
			t.Errorf("CountVowels(%q) = %d, want %d", tt.input, got, tt.want)
		}
	}
}

func TestIsPalindrome(t *testing.T) {
	tests := []struct {
		input string
		want  bool
	}{
		{"racecar", true},
		{"A man a plan a canal Panama", true},
		{"hello", false},
		{"", true},
		{"a", true},
		{"Was it a car or a cat I saw", true},
	}

	for _, tt := range tests {
		got := IsPalindrome(tt.input)
		if got != tt.want {
			t.Errorf("IsPalindrome(%q) = %v, want %v", tt.input, got, tt.want)
		}
	}
}

func TestToSnakeCase(t *testing.T) {
	tests := []struct {
		input, want string
	}{
		{"camelCase", "camel_case"},
		{"PascalCase", "pascal_case"},
		{"simple", "simple"},
		{"XMLParser", "xml_parser"},
		{"getHTTPResponse", "get_http_response"},
	}

	for _, tt := range tests {
		got := ToSnakeCase(tt.input)
		if got != tt.want {
			t.Errorf("ToSnakeCase(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

func TestTruncate(t *testing.T) {
	tests := []struct {
		s         string
		maxLength int
		suffix    string
		want      string
	}{
		{"hello world", 8, "...", "hello..."},
		{"short", 10, "...", "short"},
		{"hello world", 5, "..", "he.."},
		{"", 5, "...", ""},
		{"exact", 5, "...", "exact"},
	}

	for _, tt := range tests {
		got := Truncate(tt.s, tt.maxLength, tt.suffix)
		if got != tt.want {
			t.Errorf("Truncate(%q, %d, %q) = %q, want %q",
				tt.s, tt.maxLength, tt.suffix, got, tt.want)
		}
	}
}
