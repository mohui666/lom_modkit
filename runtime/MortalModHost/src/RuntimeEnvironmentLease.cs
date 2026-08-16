using System;

namespace MortalModHost
{
    /// <summary>
    /// Tracks ownership of a reusable runtime environment. The state is detached before
    /// reset callbacks run, so a throwing cleanup cannot leave a stale environment active.
    /// </summary>
    internal sealed class RuntimeEnvironmentLease<T> where T : class
    {
        private T _active;
        private string _ownerId;

        internal T Active { get { return _active; } }
        internal string OwnerId { get { return _ownerId; } }

        /// <summary>
        /// Same environment and owner preserve session globals. Any other transition resets
        /// the old environment, resets the incoming environment, then establishes ownership.
        /// </summary>
        internal bool Prepare(T environment, string ownerId, Action<T> reset)
        {
            if (environment == null) throw new ArgumentNullException("environment");
            if (reset == null) throw new ArgumentNullException("reset");
            if (ReferenceEquals(_active, environment)
                && string.Equals(_ownerId, ownerId, StringComparison.Ordinal))
                return false;

            T previous = Detach();
            if (previous != null)
                reset(previous);
            if (!ReferenceEquals(previous, environment))
                reset(environment);
            _active = environment;
            _ownerId = ownerId;
            return true;
        }

        /// <summary>Detach first, then reset. Repeated release is intentionally a no-op.</summary>
        internal void Release(Action<T> reset)
        {
            if (reset == null) throw new ArgumentNullException("reset");
            T environment = Detach();
            if (environment != null) reset(environment);
        }

        /// <summary>Forget ownership without invoking cleanup; used by callers doing staged cleanup.</summary>
        internal T Detach()
        {
            T environment = _active;
            _active = null;
            _ownerId = null;
            return environment;
        }
    }
}
