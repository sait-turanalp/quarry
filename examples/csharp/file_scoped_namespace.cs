/// <summary>
/// C# 10+ file-scoped namespace example.
/// Tests: file_scoped_namespace_declaration node type
/// </summary>

using System;

namespace Codanna.Examples.FileScopedNamespace;

/// <summary>
/// Example class using file-scoped namespace syntax (C# 10+).
/// No curly braces needed for the namespace.
/// </summary>
public class FileScope EdClass
{
    /// <summary>
    /// Example method.
    /// </summary>
    public void DoSomething()
    {
        Console.WriteLine("File-scoped namespace example");
    }
}

/// <summary>
/// Another class in the same file-scoped namespace.
/// </summary>
public interface IFileScopedService
{
    void Execute();
}
