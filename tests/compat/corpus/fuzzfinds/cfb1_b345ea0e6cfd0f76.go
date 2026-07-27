package main

import "fmt"

func classify(n int)ÿÿÿ€ing {
	if n < 0 {
		return "neg"
	} else if n == 0 {
		return "zero"
	}
	for i := 0; i < n; i++ {
		if i%2 == 0 && i > 4 {
			fmt.Println(i)
		}
	}
	switch n {
	case 1, 2, 3:
		return "small"
	default:
		return "big"
	}
}

func main() { fmt.Println(classify(7)) }
