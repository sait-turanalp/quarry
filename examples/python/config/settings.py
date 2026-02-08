"""Application configuration settings."""

import os
from typing import Optional, Dict, Any


class Settings:
    """Application settings management."""
    
    def __init__(self):
        """Initialize default settings."""
        self.debug = False
        self.db_url = "sqlite:///app.db"
        self.port = 8080
        self.host = "localhost"
        self._env_vars: Dict[str, str] = {}
        
    def load_from_env(self) -> None:
        """Load settings from environment variables."""
        self.debug = os.getenv("DEBUG", "false").lower() == "true"
        self.db_url = os.getenv("DATABASE_URL", self.db_url)
        self.port = int(os.getenv("PORT", str(self.port)))
        self.host = os.getenv("HOST", self.host)
        
    def get(self, key: str, default: Optional[Any] = None) -> Any:
        """Get a setting value.
        
        Args:
            key: Setting key
            default: Default value if key not found
            
        Returns:
            Setting value or default
        """
        return getattr(self, key, default)
        
    def set(self, key: str, value: Any) -> None:
        """Set a setting value.
        
        Args:
            key: Setting key
            value: Setting value
        """
        setattr(self, key, value)
        
    def __repr__(self) -> str:
        """Developer representation."""
        return f"Settings(debug={self.debug}, db_url='{self.db_url}', port={self.port})"


# Global settings instance
default_settings = Settings()