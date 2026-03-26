package calculator

import (
	"testing"
)

func TestAdd(t *testing.T) {
	tests := []struct {
		a, b, want int
	}{
		{2, 3, 5},
		{-1, 1, 0},
		{0, 0, 0},
		{100, 200, 300},
	}

	for _, tt := range tests {
		got := Add(tt.a, tt.b)
		if got != tt.want {
			t.Errorf("Add(%d, %d) = %d, want %d", tt.a, tt.b, got, tt.want)
		}
	}
}

func TestSubtract(t *testing.T) {
	tests := []struct {
		a, b, want int
	}{
		{5, 3, 2},
		{0, 5, -5},
		{10, 10, 0},
	}

	for _, tt := range tests {
		got := Subtract(tt.a, tt.b)
		if got != tt.want {
			t.Errorf("Subtract(%d, %d) = %d, want %d", tt.a, tt.b, got, tt.want)
		}
	}
}

func TestMultiply(t *testing.T) {
	tests := []struct {
		a, b, want int
	}{
		{3, 4, 12},
		{-2, 3, -6},
		{0, 100, 0},
	}

	for _, tt := range tests {
		got := Multiply(tt.a, tt.b)
		if got != tt.want {
			t.Errorf("Multiply(%d, %d) = %d, want %d", tt.a, tt.b, got, tt.want)
		}
	}
}

func TestDivide(t *testing.T) {
	tests := []struct {
		a, b   int
		want   float64
		hasRes bool
	}{
		{10, 2, 5.0, true},
		{7, 2, 3.5, true},
		{10, 0, 0, false},
	}

	for _, tt := range tests {
		got := Divide(tt.a, tt.b)
		if tt.hasRes {
			if got == nil {
				t.Errorf("Divide(%d, %d) = nil, want %f", tt.a, tt.b, tt.want)
			} else if *got != tt.want {
				t.Errorf("Divide(%d, %d) = %f, want %f", tt.a, tt.b, *got, tt.want)
			}
		} else {
			if got != nil {
				t.Errorf("Divide(%d, %d) = %f, want nil", tt.a, tt.b, *got)
			}
		}
	}
}
