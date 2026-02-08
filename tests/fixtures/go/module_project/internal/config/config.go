package config

type Config struct {
    Port int
    Host string
}

func New() *Config {
    return &Config{
        Port: 8080,
        Host: "localhost",
    }
}

func (c *Config) String() string {
    return fmt.Sprintf("%s:%d", c.Host, c.Port)
}