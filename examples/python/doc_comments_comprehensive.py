"""Module-level docstring using triple double quotes.

This is the first docstring in the file and should be treated as module documentation.
It spans multiple lines and describes the module's purpose.

This module contains comprehensive examples of all Python docstring patterns
to test and validate the enhanced docstring extraction capabilities.
"""

# Standard library imports (no docstrings)
import os
import sys
from typing import Dict, List, Optional, Union

# Module-level constant with trailing docstring
MODULE_CONSTANT = 42
"""Documentation for a module-level constant.

This trailing docstring should be associated with MODULE_CONSTANT.
"""

# Global variable with documentation
GLOBAL_VAR: str = "example"
"""Global variable documentation using triple double quotes."""

ANOTHER_GLOBAL = ["item1", "item2"]
'''Global variable documentation using triple single quotes.

Should handle both """ and ''' formats correctly.
'''


def function_with_docstring():
    """Standard function docstring.
    
    This is a function with proper docstring formatting.
    Should be extracted correctly (existing functionality).
    
    Returns:
        None: This function doesn't return anything.
    """
    pass


def function_with_single_quotes():
    '''Function docstring using triple single quotes.
    
    Should handle both """ and ''' formats for function docstrings.
    '''
    pass


def function_with_raw_docstring():
    r"""Raw docstring with escape sequences.
    
    This is a raw docstring that might contain \n and other escapes.
    The raw string prefix should be handled correctly.
    """
    pass


@property
def decorated_function():
    """Docstring for a decorated function.
    
    This tests decorator-aware parsing. The docstring should be found
    even though there's a @property decorator before the function.
    """
    return "example"


@staticmethod  
@classmethod
def multiple_decorators():
    """Function with multiple decorators.
    
    This tests that docstring extraction works with multiple decorators.
    """
    pass


def function_no_docstring():
    # Regular comment, not a docstring
    return "no docs"


class StandardClass:
    """Standard class docstring.
    
    This should be extracted correctly (existing functionality).
    Contains methods with various docstring patterns.
    """
    
    def __init__(self, value: int):
        """Constructor docstring.
        
        This method docstring should be extracted (currently missing).
        
        Args:
            value: An integer value for initialization.
        """
        self.value = value
        
    def instance_method(self):
        """Instance method docstring.
        
        This method docstring should be extracted (currently missing).
        This is one of the major gaps we need to fill.
        """
        return self.value
    
    def instance_method_single_quotes(self):
        '''Instance method with single quote docstring.
        
        Should handle both """ and ''' formats for method docstrings.
        '''
        return self.value * 2
        
    @classmethod
    def class_method(cls):
        """Class method docstring.
        
        This decorated method should have its docstring extracted.
        """
        return cls()
    
    @staticmethod
    def static_method():
        """Static method docstring.
        
        This static method should have its docstring extracted.
        """
        return "static"
        
    @property
    def documented_property(self):
        """Property docstring.
        
        This property should have its docstring extracted.
        Properties are methods with special decorators.
        """
        return self._value
    
    @documented_property.setter
    def documented_property(self, value):
        """Property setter docstring.
        
        Setter methods should also have docstring extraction.
        """
        self._value = value
        
    def method_no_docstring(self):
        # Regular comment, not a docstring
        return "no docs"


class ClassWithSingleQuotes:
    '''Class docstring using triple single quotes.
    
    Should handle both """ and ''' formats for class docstrings.
    '''
    
    def method_in_single_quote_class(self):
        """Method in class with single quote class docstring.
        
        Mixed quote usage should work correctly.
        """
        pass


class EmptyClass:
    """Empty class with only docstring."""
    pass


class ClassNoDocstring:
    # Regular comment, not a docstring
    pass


# Nested class example
class OuterClass:
    """Outer class docstring."""
    
    def outer_method(self):
        """Outer method docstring."""
        pass
        
    class InnerClass:
        """Inner class docstring.
        
        Nested classes should have their docstrings extracted.
        """
        
        def inner_method(self):
            """Inner method docstring.
            
            Methods in nested classes should be handled.
            """
            pass


# Function with complex signature
def complex_function(
    param1: str,
    param2: Optional[int] = None,
    *args: Union[str, int],
    **kwargs: Dict[str, any]
) -> List[str]:
    """Function with complex signature and docstring.
    
    This function has a complex signature with type hints,
    default values, *args, and **kwargs.
    
    Args:
        param1: A string parameter.
        param2: An optional integer parameter.
        *args: Variable positional arguments.
        **kwargs: Variable keyword arguments.
        
    Returns:
        A list of strings.
        
    Raises:
        ValueError: If param1 is empty.
    """
    if not param1:
        raise ValueError("param1 cannot be empty")
    return [param1] + [str(arg) for arg in args]


# Edge cases and special patterns
def function_with_immediate_docstring():"""Docstring immediately after colon."""
pass

def function_with_comment_before_docstring():
    # This is a regular comment
    """This docstring comes after a comment.
    
    Should still be extracted as the first string literal.
    """
    pass

# Module-level nested function
def outer_function():
    """Outer function docstring."""
    
    def inner_function():
        """Inner function docstring.
        
        Nested functions should have docstring extraction.
        """
        return "inner"
    
    return inner_function


# Variable annotations with docstrings
annotated_var: int = 100
"""Documentation for an annotated variable."""

# Multiple assignment (edge case)
a, b, c = 1, 2, 3
"""Documentation for multiple assignment.

This might be an edge case that's tricky to handle.
"""

if __name__ == "__main__":
    """This is not a proper docstring location.
    
    This should not be extracted as a docstring since it's 
    inside a conditional block.
    """
    print("Running comprehensive docstring examples")


# File ends with a docstring (edge case)
def final_function():
    """Final function in the file.
    
    This tests that docstring extraction works for the last function.
    """
    return "final"