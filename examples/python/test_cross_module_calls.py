"""Test file for cross-module call tracking in Python."""

def init_config_file():
    """Initialize configuration file.

    This function sets up the initial configuration
    and calls the nested class method to initialize
    global directories.
    """
    # This should be tracked as calling app.init.init_global_dirs
    app.init.init_global_dirs()

def init_global_dirs():
    """Initialize global directories.

    This is a module-level function that initializes
    local directories (not the nested class version).
    """
    print("Initializing global directories")

# Simulated module path call
class app:
    class init:
        @staticmethod
        def init_global_dirs():
            """Initialize global directories in app.init module.

            This static method handles the initialization of global
            directories for the application. It's called via the
            fully qualified path app.init.init_global_dirs().

            Note: This is a @staticmethod so no self/cls parameter.
            """
            print("Initializing from app.init module")