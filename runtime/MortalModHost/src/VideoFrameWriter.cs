using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.ExceptionServices;
using System.Threading;

namespace MortalModHost
{
    // One producer fills rented buffers, then transfers ownership with Submit.
    // Complete is required before publishing a movie; Dispose cancels instead.
    // The owned stream must unblock writes when closed. For ffmpeg's synchronous
    // stdin pipe, kill the encoder BEFORE Dispose on cancellation/error so a
    // blocked OS pipe write (or its Close) can finish without waiting for input.
    internal sealed class VideoFrameWriter : IDisposable
    {
        private const int WaitTimeoutMilliseconds = 10000;
        private enum BufferState { Available, Rented, Queued, Writing }
        private readonly object _gate = new object();
        private readonly Stream _output;
        private readonly byte[][] _buffers;
        private readonly BufferState[] _states;
        private readonly Queue<int> _available;
        private readonly Queue<int> _pending;
        private readonly Thread _worker;
        private ExceptionDispatchInfo _failure;
        private bool _completing, _aborting, _disposed;
        private int _outputClosed;

        internal VideoFrameWriter(Stream output, int frameBytes, int capacity = 3)
        {
            if (output == null) throw new ArgumentNullException(nameof(output));
            if (!output.CanWrite) throw new ArgumentException("视频输出流不可写", nameof(output));
            if (frameBytes <= 0) throw new ArgumentOutOfRangeException(nameof(frameBytes));
            if (capacity < 1 || capacity > 3) throw new ArgumentOutOfRangeException(nameof(capacity), "视频帧缓冲数量须在 1～3 之间");
            _output = output;
            _buffers = new byte[capacity][];
            _states = new BufferState[capacity];
            _available = new Queue<int>(capacity);
            _pending = new Queue<int>(capacity);
            for (int index = 0; index < capacity; index++)
            {
                _buffers[index] = new byte[frameBytes];
                _available.Enqueue(index);
            }
            _worker = new Thread(WriteFrames) { IsBackground = true, Name = "LoM video frame writer" };
            _worker.Start();
        }

        internal byte[] Rent()
        {
            lock (_gate)
            {
                int started = Environment.TickCount;
                while (true)
                {
                    CheckLocked();
                    if (_completing) throw new InvalidOperationException("视频帧写入已经结束");
                    if (_available.Count != 0)
                    {
                        int index = _available.Dequeue();
                        _states[index] = BufferState.Rented;
                        return _buffers[index];
                    }
                    int remaining = WaitTimeoutMilliseconds - unchecked(Environment.TickCount - started);
                    if (remaining <= 0)
                        throw new TimeoutException("视频编码器长时间未消费帧；请终止编码器后释放帧写入器");
                    Monitor.Wait(_gate, remaining);
                }
            }
        }

        internal void Submit(byte[] frame)
        {
            lock (_gate)
            {
                CheckLocked();
                if (_completing) throw new InvalidOperationException("视频帧写入已经结束");
                int index = Array.IndexOf(_buffers, frame);
                if (index < 0 || _states[index] != BufferState.Rented)
                    throw new ArgumentException("提交的帧必须由此写入器租借且尚未提交", nameof(frame));
                _states[index] = BufferState.Queued;
                _pending.Enqueue(index);
                Monitor.PulseAll(_gate);
            }
        }

        internal void Check()
        {
            lock (_gate) CheckLocked();
        }

        internal void Complete()
        {
            lock (_gate)
            {
                CheckLocked();
                foreach (BufferState state in _states)
                    if (state == BufferState.Rented)
                        throw new InvalidOperationException("视频仍有已租借但未提交的帧");
                _completing = true;
                Monitor.PulseAll(_gate);
            }
            if (!_worker.Join(WaitTimeoutMilliseconds))
                throw new TimeoutException("视频帧未能排空；请终止编码器后释放帧写入器");
            Check();
        }

        public void Dispose()
        {
            lock (_gate)
            {
                if (_disposed) return;
                _aborting = true;
                Monitor.PulseAll(_gate);
            }
            // Never hold the pool lock while closing the pipe or joining the
            // writer: its failure/exit path needs that same lock to wake Rent.
            ExceptionDispatchInfo closeFailure = null;
            try { CloseOutput(); }
            catch (Exception error) { closeFailure = ExceptionDispatchInfo.Capture(error); }
            if (!_worker.Join(WaitTimeoutMilliseconds))
                throw new TimeoutException("视频写入线程尚未退出；请先终止编码器并关闭其 stdin，再释放写入器");
            lock (_gate) { _disposed = true; Monitor.PulseAll(_gate); }
            if (closeFailure != null) closeFailure.Throw();
        }

        private void WriteFrames()
        {
            try
            {
                while (true)
                {
                    int index;
                    lock (_gate)
                    {
                        while (_pending.Count == 0 && !_completing && !_aborting) Monitor.Wait(_gate);
                        if (_aborting || _pending.Count == 0 && _completing) return;
                        index = _pending.Dequeue();
                        _states[index] = BufferState.Writing;
                    }
                    _output.Write(_buffers[index], 0, _buffers[index].Length);
                    lock (_gate)
                    {
                        _states[index] = BufferState.Available;
                        _available.Enqueue(index);
                        Monitor.PulseAll(_gate);
                    }
                }
            }
            catch (Exception error) { Failed(error); }
            finally
            {
                try { CloseOutput(); }
                catch (Exception error) { Failed(error); }
                lock (_gate) Monitor.PulseAll(_gate);
            }
        }

        private void Failed(Exception error)
        {
            lock (_gate)
            {
                if (!_aborting && _failure == null) _failure = ExceptionDispatchInfo.Capture(error);
                Monitor.PulseAll(_gate);
            }
        }

        private void CheckLocked()
        {
            if (_failure != null) _failure.Throw();
            if (_aborting || _disposed) throw new ObjectDisposedException(nameof(VideoFrameWriter));
        }

        private void CloseOutput()
        {
            if (Interlocked.Exchange(ref _outputClosed, 1) == 0) _output.Dispose();
        }
    }
}
