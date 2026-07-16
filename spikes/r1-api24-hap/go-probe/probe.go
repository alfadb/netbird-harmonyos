package main

/*
#include <stdint.h>
*/
import "C"

import (
	"net"
	"runtime"
	"strconv"
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

	result := make(chan int, 1)
	go func(size int) {
		timer := time.NewTimer(time.Millisecond)
		<-timer.C

		buffer := make([]byte, size)
		for index := range buffer {
			buffer[index] = byte(index)
		}
		result <- len(buffer)
		runtime.KeepAlive(buffer)
	}(int(allocBytes))

	return C.longlong(<-result)
}

//export NetDialProbe
func NetDialProbe(host *C.char, port C.int) C.int {
	if host == nil || port <= 0 || port > 65535 {
		return -2
	}

	address := net.JoinHostPort(C.GoString(host), strconv.Itoa(int(port)))
	connection, err := net.DialTimeout("tcp", address, 2*time.Second)
	if err != nil {
		return -1
	}
	_ = connection.Close()
	return 0
}

func main() {}
