package utils

import "strings"

func Process(data string) string {
    return strings.ToUpper(data)
}

func Sanitize(input string) string {
    return strings.TrimSpace(input)
}