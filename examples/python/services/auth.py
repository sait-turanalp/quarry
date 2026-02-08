"""Authentication service module."""

from typing import Optional, Dict
from models.user import User


class AuthService:
    """Service for handling authentication."""
    
    def __init__(self, database):
        """Initialize auth service with database connection."""
        self.database = database
        self._users: Dict[str, User] = {}
        self._sessions: Dict[str, str] = {}
        
    def register_user(self, user: User) -> bool:
        """Register a new user.
        
        Args:
            user: The user to register
            
        Returns:
            True if registration successful
        """
        if user.email in self._users:
            return False
        self._users[user.email] = user
        return True
        
    def authenticate(self, email: str, password: str) -> Optional[str]:
        """Authenticate a user and create a session.
        
        Args:
            email: User's email
            password: User's password
            
        Returns:
            Session token if authentication successful, None otherwise
        """
        if email not in self._users:
            return None
        # Simple mock authentication
        import uuid
        token = str(uuid.uuid4())
        self._sessions[token] = email
        return token
        
    def validate_session(self, token: str) -> Optional[User]:
        """Validate a session token.
        
        Args:
            token: Session token to validate
            
        Returns:
            User if token is valid, None otherwise
        """
        email = self._sessions.get(token)
        if email:
            return self._users.get(email)
        return None
        
    def logout(self, token: str) -> bool:
        """Logout a user by invalidating their session.
        
        Args:
            token: Session token to invalidate
            
        Returns:
            True if logout successful
        """
        if token in self._sessions:
            del self._sessions[token]
            return True
        return False