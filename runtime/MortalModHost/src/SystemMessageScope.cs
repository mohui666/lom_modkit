using System;

namespace MortalModHost
{
    /// <summary>Synchronous mutation scope; restored before any Lua wait or error escapes.</summary>
    internal static class SystemMessageScope
    {
        private static int _depth;
        internal static bool Suppressed { get { return _depth > 0; } }
        internal static T Run<T>(Func<T> change)
        {
            _depth++;
            try { return change(); }
            finally { _depth--; }
        }
    }
}
