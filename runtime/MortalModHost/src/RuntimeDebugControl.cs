namespace MortalModHost
{
    /// <summary>Node-boundary pause state for the fixed F5 development session only.</summary>
    internal static class RuntimeDebugControl
    {
        private static readonly object Gate = new object();
        private static bool _active;
        private static bool _paused;
        private static bool _pauseAtNextNode;

        internal static bool Active { get { lock (Gate) return _active; } }
        internal static bool Paused { get { lock (Gate) return _active && _paused; } }
        internal static bool PausePending { get { lock (Gate) return _active && _pauseAtNextNode && !_paused; } }

        internal static void Begin(bool development, bool continuation)
        {
            lock (Gate)
            {
                _active = development;
                if (!development || !continuation)
                {
                    _paused = false;
                    _pauseAtNextNode = false;
                }
            }
        }

        internal static void PauseBeforeNextNode()
        {
            lock (Gate) if (_active && !_paused) _pauseAtNextNode = true;
        }

        internal static void Step()
        {
            lock (Gate)
            {
                if (!_active || !_paused) return;
                _paused = false;
                _pauseAtNextNode = true;
            }
        }

        internal static void Continue()
        {
            lock (Gate)
            {
                if (!_active) return;
                _paused = false;
                _pauseAtNextNode = false;
            }
        }

        /// <summary>Called as the first statement of a node; true means yield before its body.</summary>
        internal static bool BeforeNode()
        {
            lock (Gate)
            {
                if (!_active || !_pauseAtNextNode) return false;
                _pauseAtNextNode = false;
                _paused = true;
                return true;
            }
        }

        internal static void Reset()
        {
            lock (Gate)
            {
                _active = false;
                _paused = false;
                _pauseAtNextNode = false;
            }
        }
    }
}
