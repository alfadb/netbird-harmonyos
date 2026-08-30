package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"runtime"
	"time"
)

//export Hello
func Hello() C.int {
	return 42
}

//export RuntimeProbe
func RuntimeProbe(allocBytes C.longlong) C.longlong {
	if allocBytes < 0 || allocBytes > 128*1024*1024 {
		return -1
	}

	size := int(allocBytes)
	result := make(chan int, 1)
	go func() {
		timer := time.NewTimer(time.Millisecond)
		<-timer.C

		buffer := make([]byte, size)
		for index := range buffer {
			buffer[index] = byte(index)
		}
		result <- len(buffer)
		runtime.KeepAlive(buffer)
	}()

	return C.longlong(<-result)
}

func main() {}
