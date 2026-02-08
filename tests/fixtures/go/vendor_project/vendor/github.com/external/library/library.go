// Vendored external library
package library

type Client struct {
    endpoint string
}

func NewClient(endpoint string) *Client {
    return &Client{endpoint: endpoint}
}

func (c *Client) Connect() error {
    // Vendored implementation
    return nil
}

func (c *Client) GetData() ([]byte, error) {
    // Vendored implementation
    return []byte("vendored data"), nil
}