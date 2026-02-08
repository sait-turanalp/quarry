"""Helper utilities for the application."""

def format_output(message: str) -> str:
    """Format a message for output.
    
    Args:
        message: The message to format
        
    Returns:
        Formatted message string
    """
    return f"[OUTPUT] {message}"


def validate_input(data: str) -> bool:
    """Validate input data.
    
    Args:
        data: The data to validate
        
    Returns:
        True if valid, False otherwise
    """
    if not data:
        return False
    if "@" not in data:
        return False
    return True


class DataProcessor:
    """Process data with various transformations."""
    
    def __init__(self, config: dict):
        """Initialize processor with configuration."""
        self.config = config
        
    def process(self, data: str) -> str:
        """Process the input data."""
        return data.upper()
        
    def _internal_method(self):
        """Internal method (module visibility)."""
        pass