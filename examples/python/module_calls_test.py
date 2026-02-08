"""Test file to verify module-level class instantiation detection."""

# Module-level imports and instantiations
from database import DatabaseClient
from config import ConfigManager
from logging import Logger

# Module-level class instantiations - these should be detected as <module> -> ClassName
db_client = DatabaseClient()
config = ConfigManager()
logger = Logger("app")

# Module-level variable assignments
APP_NAME = "TestApp"
VERSION = "1.0.0"


class Application:
    """Main application class."""

    def __init__(self):
        # Method-level instantiations
        self.cache = CacheManager()
        self.auth = AuthenticationService()

    def initialize(self):
        """Initialize the application."""
        # Function-level instantiations
        validator = InputValidator()
        processor = DataProcessor()
        return processor.process()


def main():
    """Entry point of the application."""
    # Function-level instantiations
    app = Application()
    router = Router()
    server = Server()

    # Start the application
    server.run(app, router)


# Module-level instantiation at the end
error_handler = ErrorHandler()

if __name__ == "__main__":
    main()