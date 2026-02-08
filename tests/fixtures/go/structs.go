// Package structs demonstrates Go struct declarations and methods
package structs

import (
	"fmt"
	"time"
)

// Basic struct with exported and unexported fields
type User struct {
	ID       int64     `json:"id" db:"user_id"`
	Name     string    `json:"name" validate:"required"`
	Email    string    `json:"email" validate:"email"`
	age      int       `json:"-"`
	verified bool
	Created  time.Time `json:"created_at"`
}

// Struct with different field types
type Product struct {
	ID          string
	Name        string
	Price       float64
	Categories  []string
	Metadata    map[string]interface{}
	Dimensions  struct {
		Width  float64
		Height float64
		Depth  float64
	}
	IsActive *bool // Pointer to bool for nullable field
}

// Empty struct
type Signal struct{}

// Struct with anonymous fields (embedding)
type Person struct {
	User          // Embedded struct
	FirstName string
	LastName  string
	Address   Address
}

// Nested struct
type Address struct {
	Street  string
	City    string
	State   string
	ZipCode string
	Country string
}

// Struct with function fields
type Handler struct {
	Name     string
	Callback func(data interface{}) error
	Validator func(input string) bool
}

// Constructor function for User
func NewUser(name, email string) *User {
	return &User{
		Name:    name,
		Email:   email,
		Created: time.Now(),
		verified: false,
	}
}

// Method with value receiver
func (u User) GetDisplayName() string {
	return fmt.Sprintf("%s <%s>", u.Name, u.Email)
}

// Method with pointer receiver
func (u *User) SetAge(age int) {
	u.age = age
}

// Method with pointer receiver that modifies the struct
func (u *User) Verify() {
	u.verified = true
}

// Method with return values
func (u User) IsVerified() bool {
	return u.verified
}

// Method with multiple parameters
func (u *User) UpdateInfo(name, email string) error {
	if name == "" || email == "" {
		return fmt.Errorf("name and email cannot be empty")
	}
	u.Name = name
	u.Email = email
	return nil
}

// Method on Product struct
func (p Product) GetFullName() string {
	return fmt.Sprintf("%s (%s)", p.Name, p.ID)
}

// Method with pointer receiver on Product
func (p *Product) SetPrice(price float64) {
	p.Price = price
}

// Method that returns multiple values
func (p Product) GetDimensions() (float64, float64, float64) {
	return p.Dimensions.Width, p.Dimensions.Height, p.Dimensions.Depth
}

// Method on embedded struct
func (p Person) GetFullName() string {
	return fmt.Sprintf("%s %s", p.FirstName, p.LastName)
}

// Method that accesses embedded struct fields
func (p Person) GetUserInfo() string {
	return p.User.GetDisplayName()
}

// Method on Address struct
func (a Address) GetFullAddress() string {
	return fmt.Sprintf("%s, %s, %s %s, %s", 
		a.Street, a.City, a.State, a.ZipCode, a.Country)
}

// Method with interface{} parameter
func (h Handler) Execute(data interface{}) error {
	if h.Validator != nil {
		if input, ok := data.(string); ok {
			if !h.Validator(input) {
				return fmt.Errorf("validation failed")
			}
		}
	}
	return h.Callback(data)
}

// Factory function for Product
func NewProduct(id, name string, price float64) Product {
	return Product{
		ID:    id,
		Name:  name,
		Price: price,
		Categories: make([]string, 0),
		Metadata:   make(map[string]interface{}),
	}
}

// Factory function with complex initialization
func NewHandler(name string, callback func(data interface{}) error) *Handler {
	return &Handler{
		Name:     name,
		Callback: callback,
		Validator: func(input string) bool {
			return len(input) > 0
		},
	}
}

// Function that operates on structs
func ProcessUsers(users []User) []string {
	names := make([]string, len(users))
	for i, user := range users {
		names[i] = user.GetDisplayName()
	}
	return names
}

// Function that takes struct by pointer
func VerifyUser(user *User) {
	user.Verify()
}

// Function with struct return type
func CreateDefaultUser() User {
	return User{
		Name:    "Anonymous",
		Email:   "anon@example.com",
		Created: time.Now(),
	}
}

// Function with multiple struct parameters
func CopyUserInfo(source User, dest *User) {
	dest.Name = source.Name
	dest.Email = source.Email
	dest.age = source.age
}

// Function that creates struct with anonymous fields
func CreatePerson(firstName, lastName, email string) Person {
	user := NewUser(fmt.Sprintf("%s %s", firstName, lastName), email)
	return Person{
		User:      *user,
		FirstName: firstName,
		LastName:  lastName,
		Address: Address{
			Country: "Unknown",
		},
	}
}