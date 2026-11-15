using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Threading;

namespace MortalModHost
{
    internal static class VideoFrameWriterTests
    {
        internal static void Run()
        {
            OrderedFrames();
            BoundedRentWaitsForReuse();
            FailureWakesProducer();
            ClosingUnblocksBothThreads();
            Console.WriteLine("PASS video writer FIFO, bounded buffer reuse, real-thread failure and cancellation cleanup (offline)");
        }

        private static void OrderedFrames()
        {
            var output = new MemoryStream();
            var writer = new VideoFrameWriter(output, 4);
            var rented = new HashSet<byte[]>();
            try
            {
                for (int index = 0; index < 64; index++)
                {
                    byte[] frame = writer.Rent();
                    rented.Add(frame); Fill(frame, index);
                    writer.Submit(frame);
                }
                writer.Complete();
                writer.Check();
                byte[] bytes = output.ToArray();
                Check(bytes.Length == 64 * 4 && rented.Count <= 3, "frame count or buffer limit changed");
                for (int index = 0; index < 64; index++)
                    Check(bytes[index * 4] == index && bytes[index * 4 + 1] == 255 - index
                        && bytes[index * 4 + 2] == (index ^ 170) && bytes[index * 4 + 3] == 42, "frames were reordered or overwritten");
                Check(!output.CanWrite, "Complete did not close owned stdin");
            }
            finally { writer.Dispose(); }
            Stopped(writer);
        }

        private static void BoundedRentWaitsForReuse()
        {
            var output = new GateStream();
            var writer = new VideoFrameWriter(output, 4);
            var buffers = new HashSet<byte[]>();
            var producerStarted = new ManualResetEventSlim();
            var producerDone = new ManualResetEventSlim();
            Exception failure = null;
            byte[] fourth = null;
            var producer = new Thread(() =>
            {
                producerStarted.Set();
                try { fourth = writer.Rent(); Fill(fourth, 3); writer.Submit(fourth); }
                catch (Exception error) { failure = error; }
                finally { producerDone.Set(); }
            }) { IsBackground = true };
            try
            {
                for (int index = 0; index < 3; index++)
                {
                    byte[] frame = writer.Rent(); buffers.Add(frame); Fill(frame, index); writer.Submit(frame);
                }
                Check(output.Entered.Wait(5000), "writer did not start");
                producer.Start();
                Check(producerStarted.Wait(5000) && !producerDone.Wait(50), "fourth rent escaped the three-buffer limit");
                output.Release.Set();
                Check(producer.Join(5000) && failure == null && buffers.Contains(fourth), "released frame was not reused");
                writer.Complete();
                Check(output.Bytes.Length == 16, "bounded producer lost a submitted frame");
            }
            finally
            {
                output.Release.Set(); writer.Dispose();
                if (producer.IsAlive) Check(producer.Join(5000), "producer leaked");
                output.CloseEvents(); producerStarted.Dispose(); producerDone.Dispose();
            }
            Stopped(writer);
        }

        private static void FailureWakesProducer()
        {
            var expected = new IOException("expected frame pipe failure");
            var output = new GateStream { Failure = expected };
            var writer = new VideoFrameWriter(output, 4, 1);
            var producerStarted = new ManualResetEventSlim();
            var producerDone = new ManualResetEventSlim();
            Exception failure = null;
            var producer = new Thread(() =>
            {
                producerStarted.Set();
                try { writer.Rent(); }
                catch (Exception error) { failure = error; }
                finally { producerDone.Set(); }
            }) { IsBackground = true };
            try
            {
                writer.Submit(writer.Rent());
                Check(output.Entered.Wait(5000), "failure writer did not start");
                producer.Start();
                Check(producerStarted.Wait(5000) && !producerDone.Wait(50), "producer did not wait for the occupied buffer");
                output.Release.Set();
                Check(producer.Join(5000) && ReferenceEquals(failure, expected), "write failure did not wake and fail Rent");
                ThrowsSame(writer.Check, expected);
                ThrowsSame(writer.Complete, expected);
            }
            finally
            {
                output.Release.Set(); writer.Dispose();
                if (producer.IsAlive) Check(producer.Join(5000), "failed producer leaked");
                output.CloseEvents(); producerStarted.Dispose(); producerDone.Dispose();
            }
            Stopped(writer);
        }

        private static void ClosingUnblocksBothThreads()
        {
            var output = new GateStream();
            var writer = new VideoFrameWriter(output, 4, 1);
            var started = new ManualResetEventSlim();
            Exception failure = null;
            var producer = new Thread(() =>
            {
                started.Set();
                try { writer.Rent(); }
                catch (Exception error) { failure = error; }
            }) { IsBackground = true };
            try
            {
                writer.Submit(writer.Rent());
                Check(output.Entered.Wait(5000), "cancel writer did not start");
                producer.Start(); Check(started.Wait(5000), "cancel producer did not start");
                writer.Dispose();
                Check(producer.Join(5000) && failure is ObjectDisposedException, "cancel did not release blocked Rent");
                Stopped(writer);
            }
            finally
            {
                output.Release.Set(); writer.Dispose();
                if (producer.IsAlive) Check(producer.Join(5000), "cancel producer leaked");
                output.CloseEvents(); started.Dispose();
            }
        }

        private sealed class GateStream : Stream
        {
            internal readonly ManualResetEventSlim Entered = new ManualResetEventSlim();
            internal readonly ManualResetEventSlim Release = new ManualResetEventSlim();
            private readonly MemoryStream _bytes = new MemoryStream();
            private volatile bool _closed;
            internal IOException Failure;
            internal byte[] Bytes { get { return _bytes.ToArray(); } }
            public override bool CanRead { get { return false; } }
            public override bool CanSeek { get { return false; } }
            public override bool CanWrite { get { return !_closed; } }
            public override long Length { get { throw new NotSupportedException(); } }
            public override long Position { get { throw new NotSupportedException(); } set { throw new NotSupportedException(); } }
            public override void Flush() { }
            public override int Read(byte[] buffer, int offset, int count) { throw new NotSupportedException(); }
            public override long Seek(long offset, SeekOrigin origin) { throw new NotSupportedException(); }
            public override void SetLength(long value) { throw new NotSupportedException(); }
            public override void Write(byte[] buffer, int offset, int count)
            {
                Entered.Set(); Release.Wait();
                if (_closed) throw new ObjectDisposedException(nameof(GateStream));
                if (Failure != null) throw Failure;
                _bytes.Write(buffer, offset, count);
            }
            protected override void Dispose(bool disposing) { _closed = true; Release.Set(); _bytes.Dispose(); base.Dispose(disposing); }
            internal void CloseEvents() { Entered.Dispose(); Release.Dispose(); }
        }

        private static void Fill(byte[] frame, int index)
        { frame[0] = (byte)index; frame[1] = (byte)(255 - index); frame[2] = (byte)(index ^ 170); frame[3] = 42; }
        private static void Stopped(VideoFrameWriter writer)
        {
            var thread = (Thread)typeof(VideoFrameWriter).GetField("_worker", BindingFlags.Instance | BindingFlags.NonPublic).GetValue(writer);
            Check(!thread.IsAlive, "video writer thread survived cleanup");
        }
        private static void ThrowsSame(Action action, Exception expected)
        { try { action(); } catch (Exception error) { Check(ReferenceEquals(error, expected), "original output error was replaced"); return; } throw new Exception("expected output error was not propagated"); }
        private static void Check(bool condition, string message) { if (!condition) throw new Exception(message); }
    }
}
