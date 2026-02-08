// Test file for qualified names functionality
package test

// Test struct with fields for qualification
type Person struct {
	Name string
	Age  int
}

// Test interface with methods for qualification  
type Reader interface {
	Read(data []byte) (int, error)
	Close() error
}

// Another struct with same field names as Person (to test disambiguation)
type Product struct {
	Name  string // Same name as Person.Name
	Price float64
}

// Another interface with same method names as Reader (to test disambiguation)
type Writer interface {
	Write(data []byte) (int, error) 
	Close() error // Same name as Reader.Close
}