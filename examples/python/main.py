#!/usr/bin/env python3
"""Main entry point for the Python example application."""

import os
import sys
from typing import List, Optional

# Import from local modules
from utils.helper import format_output, validate_input
from models.user import User, UserRole
from services.auth import AuthService
from services.database import DatabaseConnection

# Import with alias
from config.settings import Settings as AppSettings
import logging as log


def main():
    """Main application entry point."""
    log.info("Starting application")
    
    # Initialize configuration
    settings = AppSettings()
    settings.load_from_env()
    
    # Setup database
    db = DatabaseConnection(settings.db_url)
    db.connect()
    
    # Create auth service
    auth = AuthService(db)
    
    # Example user
    user = User(
        name="John Doe",
        email="john@example.com",
        role=UserRole.ADMIN
    )
    
    if validate_input(user.email):
        auth.register_user(user)
        output = format_output(f"User {user.name} registered")
        print(output)
    
    db.close()
    log.info("Application finished")


if __name__ == "__main__":
    main()