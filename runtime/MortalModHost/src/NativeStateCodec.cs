using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace MortalModHost
{
    internal static unsafe class NativeStateCodec
    {
        private static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate uint Transform(char* source, int length, char* output, int capacity, int* written);
        private static readonly Transform Encode = NativeHostKernel.Bind<Transform>("lom_state_encode_utf16_v1");
        private static readonly Transform Decode = NativeHostKernel.Bind<Transform>("lom_state_decode_utf16_v1");
        internal static bool IsAvailable { get { return Encode != null; } }

        // This is the existing namespace scan: no writes or cross-dispatch cache.
        internal static string ReadRecord(IEnumerable<string> keys, string prefix, string packageId)
        {
            string record = null;
            foreach (string key in keys)
            {
                if (key == null || !key.StartsWith(prefix, StringComparison.Ordinal)) continue;
                if (record != null) throw new InvalidOperationException("事件存档存在重复记录：" + packageId);
                record = key;
            }
            return record == null ? null : DecodeRecord(record, prefix.Length);
        }

        internal static string EncodeManaged(string prefix, string json)
        {
            byte[] bytes = Utf8.GetBytes(json);
            if (bytes.Length > 128 * 1024) throw new InvalidOperationException("事件存档超过 128 KiB");
            return prefix + Convert.ToBase64String(bytes);
        }
        internal static string DecodeManaged(string record, int offset)
        {
            string payload = offset == 0 ? record : record.Substring(offset);
            if (payload != null && payload.Length > 180000) throw new InvalidOperationException("事件存档超过上限");
            return Utf8.GetString(Convert.FromBase64String(payload));
        }
        private static void Reject(uint status, bool encode, string value, int offset)
        {
            if (status == 4) throw new InvalidOperationException("事件存档超过 128 KiB");
            if (status == 5) throw new InvalidOperationException("事件存档超过上限");
            if (status == 2 || status == 3)
            {
                // Only error diagnostics use BCL conversion. It supplies the
                // actual runtime's original exception/message/UTF index. A BCL
                // success cannot turn a native rejection into accepted data.
                if (encode) Utf8.GetBytes(value);
                else Utf8.GetString(Convert.FromBase64String(offset == 0 ? value : value.Substring(offset)));
            }
            throw new InvalidOperationException("原生事件存档编解码拒绝了请求：" + status);
        }
        internal static string EncodeRecord(string prefix, string json)
        {
            if (Encode == null || json == null) return EncodeManaged(prefix, json);
            prefix = prefix ?? "";
            int length = 0;
            fixed (char* source = json)
            {
                uint status = Encode(source, json.Length, null, 0, &length);
                if (status != 1) Reject(status, true, json, 0);
                if (length < 0 || length > 174764) throw new InvalidOperationException("原生事件存档返回无效长度");
                string result = new string('\0', prefix.Length + length);
                fixed (char* output = result)
                {
                    for (int i = 0; i < prefix.Length; ++i) output[i] = prefix[i];
                    int written = 0;
                    status = Encode(source, json.Length, output + prefix.Length, length, &written);
                    if (status != 1) Reject(status, true, json, 0);
                    if (written != length) throw new InvalidOperationException("原生事件存档返回不一致长度");
                }
                return result;
            }
        }
        internal static string DecodeRecord(string record, int offset)
        {
            if (Decode == null || record == null) return DecodeManaged(record, offset);
            if (offset < 0 || offset > record.Length) throw new ArgumentOutOfRangeException("offset");
            int sourceLength = record.Length - offset;
            if (sourceLength > 180000) throw new InvalidOperationException("事件存档超过上限");
            int length = 0;
            fixed (char* source = record)
            {
                uint status = Decode(source + offset, sourceLength, null, 0, &length);
                if (status != 1) Reject(status, false, record, offset);
                if (length < 0 || length > 135000) throw new InvalidOperationException("原生事件存档返回无效长度");
                string result = new string('\0', length);
                fixed (char* output = result)
                {
                    int written = 0;
                    status = Decode(source + offset, sourceLength, output, length, &written);
                    if (status != 1) Reject(status, false, record, offset);
                    if (written != length) throw new InvalidOperationException("原生事件存档返回不一致长度");
                }
                return result;
            }
        }
    }
}
