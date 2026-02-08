"""User model definitions."""

from enum import Enum
from typing import Optional


class UserRole(Enum):
    """User role enumeration."""
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"


class User:
    """Represents a user in the system."""
    
    def __init__(self, name: str, email: str, role: UserRole = UserRole.USER):
        """Initialize a new user.
        
        Args:
            name: User's full name
            email: User's email address
            role: User's role (defaults to USER)
        """
        self.name = name
        self.email = email
        self.role = role
        self._id: Optional[int] = None
        
    def set_id(self, user_id: int) -> None:
        """Set the user's ID."""
        self._id = user_id
        
    def get_id(self) -> Optional[int]:
        """Get the user's ID."""
        return self._id
        
    def __str__(self) -> str:
        """String representation of the user."""
        return f"User({self.name}, {self.email}, role={self.role.value})"
        
    def __repr__(self) -> str:
        """Developer representation of the user."""
        return f"User(name='{self.name}', email='{self.email}', role={self.role})"