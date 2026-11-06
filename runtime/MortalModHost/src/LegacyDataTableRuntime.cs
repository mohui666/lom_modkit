using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Linq.Expressions;
using System.Reflection;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.SceneManagement;
using UObject = UnityEngine.Object;

namespace MortalModHost
{
    /// <summary>Copy-on-write SO graphs for the original DataTable type/name/field protocol.</summary>
    internal static class LegacyDataTableRuntime
    {
        private sealed class ReferenceChange
        {
            internal object Owner, Original, Replacement;
            internal FieldInfo Field;
        }
        private sealed class EventBinding
        {
            internal ScriptableObject Source, Copy;
            internal FieldInfo Field;
            internal readonly Dictionary<Delegate, Delegate> Moved = new Dictionary<Delegate, Delegate>();
        }
        private static ModPackage _package;
        private static readonly Dictionary<UObject, ScriptableObject> Copies = new Dictionary<UObject, ScriptableObject>();
        private static readonly HashSet<UObject> Owned = new HashSet<UObject>();
        private static readonly Dictionary<GameStat, int> InitialStatValues = new Dictionary<GameStat, int>();
        private static readonly Dictionary<string, ScriptableObject> Targets = new Dictionary<string, ScriptableObject>(StringComparer.Ordinal);
        private static readonly Dictionary<object, object> ManagedCopies = new Dictionary<object, object>(ReferenceComparer.Instance);
        private static readonly HashSet<ScriptableObject> Provisional = new HashSet<ScriptableObject>();
        private static readonly List<ReferenceChange> Changes = new List<ReferenceChange>();
        private static readonly List<EventBinding> Events = new List<EventBinding>();
        private static readonly Dictionary<Type, Delegate> EmptyHandlers = new Dictionary<Type, Delegate>();
        private static readonly Dictionary<Type, FieldInfo[]> FieldCache = new Dictionary<Type, FieldInfo[]>();
        private static readonly Dictionary<string, Type> TypeCache = new Dictionary<string, Type>(StringComparer.Ordinal);
        private static readonly Assembly[] Assemblies = {
            typeof(Book).Assembly, typeof(Mortal.Story.CheckPointManager).Assembly,
            typeof(Mortal.Combat.CombatLevel).Assembly, typeof(Mortal.Battle.DropItem).Assembly,
            typeof(Mortal.Free.FreePositionData).Assembly, typeof(OBB.Framework.Data.StoryMappingItem).Assembly,
        };
        private static bool _refreshing;

        internal static void EnsurePackage(ModPackage package)
        {
            if (package == null || package.LegacyExtensions == null || package.LegacyExtensions.Tables.Count == 0) return;
            if (_package != null)
            {
                if (_package.Id != package.Id || _package.PackageFingerprint != package.PackageFingerprint)
                    throw new InvalidOperationException("当前会话已加载另一个旧 MOD 的 DataTable，请先返回标题切换作品");
                Refresh();
                return;
            }
            _package = package;
            SceneManager.sceneLoaded += SceneLoaded;
            try { Refresh(); }
            catch { Clear(); throw; }
        }

        internal static void Clear()
        {
            SceneManager.sceneLoaded -= SceneLoaded;
            RestoreListeners();
            for (int index = Changes.Count - 1; index >= 0; index--)
            {
                var change = Changes[index];
                var owner = change.Owner as UObject;
                if (change.Owner != null && owner == null && change.Owner is UObject) continue;
                if (ReferenceEquals(change.Field.GetValue(change.Owner), change.Replacement))
                    change.Field.SetValue(change.Owner, change.Original);
            }
            Changes.Clear();
            foreach (var item in Owned) if (item != null) UObject.Destroy(item);
            Owned.Clear(); Copies.Clear(); Targets.Clear(); ManagedCopies.Clear(); Provisional.Clear(); InitialStatValues.Clear(); _package = null;
        }
        internal static void ClearDevelopment()
        {
            if (_package != null && RuntimeTrace.IsDevelopmentPackage(_package)) Clear();
        }

        private static void SceneLoaded(Scene scene, LoadSceneMode mode)
        {
            if (_package == null) return;
            try { Refresh(); }
            catch (Exception error)
            {
                LuaManagerPatch.AbortActivePlayback("旧 DataTable 场景引用接入失败", error: error, category: "legacy_datatable", package: _package);
            }
        }

        private static void Refresh()
        {
            if (_package == null || _refreshing) return;
            _refreshing = true;
            try
            {
                foreach (var table in _package.LegacyExtensions.Tables)
                    Resolve(TableType(table), table.Name);
                BindLoadedSources();
                // Both directions matter. Parents must point to the session child, and
                // mutable children such as GameStat._totalAddData must stay isolated too.
                var sources = Resources.FindObjectsOfTypeAll<ScriptableObject>()
                    .Where(item => item != null && !Owned.Contains(item) && Assemblies.Contains(item.GetType().Assembly)).ToArray();
                var references = sources.ToDictionary(item => item, item => References(item).ToArray());
                bool added;
                do
                {
                    added = false;
                    foreach (var clone in Owned.OfType<ScriptableObject>().ToArray())
                    {
                        foreach (var child in References(clone).OfType<ScriptableObject>())
                        {
                            if (Owned.Contains(child) || Copies.ContainsKey(child) || !Assemblies.Contains(child.GetType().Assembly)) continue;
                            Copy(child); added = true;
                        }
                    }
                    foreach (var source in sources)
                    {
                        if (Copies.ContainsKey(source) || !references[source].Any(Copies.ContainsKey)) continue;
                        Copy(source); added = true;
                    }
                } while (added);
                foreach (var clone in Owned.OfType<ScriptableObject>().ToArray()) RemapFields(clone, false);
                foreach (var source in Copies.Keys.ToArray())
                {
                    FieldInfo singleton = OptionalField(source.GetType(), "_instance");
                    if (singleton == null || !singleton.IsStatic || !singleton.FieldType.IsInstanceOfType(Copies[source])) continue;
                    object active = singleton.GetValue(null);
                    bool firstSource = active == null && ReferenceEquals(Resources.FindObjectsOfTypeAll(source.GetType())
                        .FirstOrDefault(item => !Owned.Contains(item)), source);
                    if (ReferenceEquals(active, source) || firstSource) Replace(null, singleton, Copies[source]);
                }
                FieldInfo customDatabase = AccessTools.Field(typeof(CustomGameplayRuntime), "_database");
                var database = customDatabase.GetValue(null) as UObject;
                ScriptableObject replacement;
                if (database != null && Copies.TryGetValue(database, out replacement)) Replace(null, customDatabase, replacement);
                foreach (var component in Resources.FindObjectsOfTypeAll<MonoBehaviour>())
                {
                    if (component != null && component.gameObject.scene.IsValid()
                        && Assemblies.Contains(component.GetType().Assembly)) RemapFields(component, true);
                }
                MoveListeners();
            }
            finally { _refreshing = false; }
        }

        private static ScriptableObject Copy(ScriptableObject source)
        {
            ScriptableObject clone;
            if (Copies.TryGetValue(source, out clone)) return clone;
            clone = UObject.Instantiate(source);
            clone.name = source.name;
            Copies.Add(source, clone); Owned.Add(clone);
            // Instantiate copies serialized fields only. Live systems also keep state
            // such as ReadStorySystem._dicStoryIndex in private managed fields.
            // Retain that exact state while separating mutable managed collections.
            foreach (var field in Fields(source.GetType()))
            {
                bool serialized = !field.IsNotSerialized && (field.IsPublic || field.IsDefined(typeof(SerializeField), false));
                if (serialized || typeof(Delegate).IsAssignableFrom(field.FieldType)) continue;
                field.SetValue(clone, Remap(field.GetValue(source), ManagedCopies, true));
            }
            var stat = source as GameStat;
            if (stat != null) InitialStatValues.Add(stat, stat.Value);
            return clone;
        }

        private static void BindLoadedSources()
        {
            foreach (var table in _package.LegacyExtensions.Tables)
            {
                Type type = TableType(table);
                string key = type.FullName + "/" + table.Name;
                ScriptableObject target = Targets[key];
                foreach (var source in Resources.FindObjectsOfTypeAll(type).OfType<ScriptableObject>()
                    .Where(item => item.name == table.Name && !Owned.Contains(item) && !Copies.ContainsKey(item)))
                {
                    if (Provisional.Remove(target))
                    {
                        ScriptableObject replacement = Copy(source);
                        Targets[key] = replacement;
                        // A newly loaded official template supplies all fields omitted
                        // by a partial patch. Rewire existing custom collection links too.
                        Copies[target] = replacement;
                        target = replacement;
                        Fill(target, table.Fields, type.Name + "/" + table.Name);
                    }
                    else Copies.Add(source, target);
                }
            }
        }

        internal static string ProbeStat(string key)
        {
            GameStatType type;
            if (!OBB.Framework.Utils.EnumUtils.TryParseByStringValue(key, out type))
                throw new ArgumentException("未知测试属性：" + key);
            var current = PlayerStatManagerData.Instance.Stats.Get(type);
            if (current == null) throw new InvalidOperationException("测试属性不存在：" + key);
            var source = Copies.FirstOrDefault(pair => ReferenceEquals(pair.Value, current)).Key as GameStat ?? current;
            int before;
            if (InitialStatValues.TryGetValue(source, out before) && source.Value != before)
                throw new InvalidOperationException("旧 DataTable 隔离检查失败，原始 GameStat 被修改：" + source.name
                    + " before=" + before + " source=" + source.Value + " session=" + current.Value);
            return "GameStat/" + source.name + " source=" + source.Value + " session=" + current.Value
                + " cloned=" + (!ReferenceEquals(source, current)) + " source_unchanged="
                + (!InitialStatValues.TryGetValue(source, out before) || source.Value == before);
        }

        private static Type FindType(string name)
        {
            Type cached;
            if (TypeCache.TryGetValue(name, out cached)) return cached;
            var matches = Assemblies.SelectMany(assembly => assembly.GetTypes())
                .Where(type => type.Name == name && !type.IsAbstract && !type.ContainsGenericParameters
                    && typeof(ScriptableObject).IsAssignableFrom(type)).Distinct().ToArray();
            if (matches.Length != 1) throw new FormatException("DataTable ScriptableObject 类型不存在或不唯一：" + name);
            TypeCache.Add(name, matches[0]);
            return matches[0];
        }
        private static Type TableType(LegacyTable table)
        {
            if (table.Type != "CombatStateEffectUnitScriptable") return FindType(table.Type);
            // The old exporter puts all concrete CombatStateEffectUnitScriptable
            // subclasses in this one directory. Existing object identity supplies type.
            var existing = Resources.FindObjectsOfTypeAll<ScriptableObject>().FirstOrDefault(item => item != null
                && item.name == table.Name && BaseNamed(item.GetType(), table.Type));
            if (existing != null) return existing.GetType();
            var references = new HashSet<string>(StringComparer.Ordinal);
            foreach (var row in _package.LegacyExtensions.Tables) FindReferenceTypes(row.Fields, table.Name, references);
            var types = references.Select(FindType).Where(type => BaseNamed(type, table.Type)).ToArray();
            if (types.Length != 1) throw new FormatException("DataTable 找不到唯一的具体效果类型，请提供 so.type 引用：" + table.Type + "/" + table.Name);
            return types[0];
        }
        private static void FindReferenceTypes(object value, string name, HashSet<string> result)
        {
            var fields = value as Dictionary<string, object>;
            if (fields != null)
            {
                object objectName, type;
                if (fields.TryGetValue("so.name", out objectName) && objectName as string == name
                    && fields.TryGetValue("so.type", out type) && type is string) result.Add((string)type);
                foreach (var child in fields.Values) FindReferenceTypes(child, name, result);
                return;
            }
            var rows = value as List<object>;
            if (rows != null) foreach (var child in rows) FindReferenceTypes(child, name, result);
        }
        private static bool BaseNamed(Type type, string name)
        {
            for (; type != null; type = type.BaseType) if (type.Name == name) return true;
            return false;
        }
        private static ScriptableObject Resolve(Type type, string name)
        {
            string key = type.FullName + "/" + name;
            ScriptableObject result;
            if (Targets.TryGetValue(key, out result)) return result;
            var source = Resources.FindObjectsOfTypeAll(type).OfType<ScriptableObject>()
                .FirstOrDefault(item => item.name == name && !Owned.Contains(item));
            var table = _package.LegacyExtensions.Tables.FirstOrDefault(item => item.Name == name
                && (item.Type == type.Name || item.Type == "CombatStateEffectUnitScriptable" && BaseNamed(type, item.Type)));
            if (source == null && table == null)
                throw new FormatException("DataTable 引用不存在：" + type.Name + "/" + name);
            if (table == null)
            {
                Targets.Add(key, source);
                return source;
            }
            result = source != null ? Copy(source) : ScriptableObject.CreateInstance(type);
            if (source == null) Provisional.Add(result);
            result.name = name;
            Owned.Add(result);
            Targets.Add(key, result); // Register before fields to support cyclic SO references.
            if (table != null) Fill(result, table.Fields, type.Name + "/" + name);
            return result;
        }

        private static void Fill(object target, Dictionary<string, object> fields, string path)
        {
            foreach (var pair in fields)
            {
                if (pair.Key == "_add")
                {
                    if (!BaseNamed(target.GetType(), "CollectionData`1"))
                        throw new FormatException("DataTable _add 只用于 CollectionData：" + path);
                    FieldInfo listField = AccessTools.Field(target.GetType(), "_list");
                    var list = listField == null ? null : listField.GetValue(target) as IList;
                    var entries = pair.Value as List<object>;
                    if (list == null || entries == null) throw new FormatException("DataTable _add 必须是数组：" + path);
                    Type itemType = list.GetType().GetGenericArguments()[0];
                    foreach (object entry in entries) list.Add(ConvertValue(entry, itemType, null, path + "/_add"));
                    continue;
                }
                FieldInfo field = AccessTools.Field(target.GetType(), pair.Key);
                if (field == null || field.IsStatic) throw new FormatException("DataTable 字段不存在：" + path + "/" + pair.Key);
                field.SetValue(target, ConvertValue(pair.Value, field.FieldType, field.GetValue(target), path + "/" + pair.Key));
            }
        }

        private static object ConvertValue(object value, Type type, object previous, string path)
        {
            if (value == null)
            {
                if (type.IsValueType && Nullable.GetUnderlyingType(type) == null) throw new FormatException("DataTable 值不能为 null：" + path);
                return null;
            }
            type = Nullable.GetUnderlyingType(type) ?? type;
            if (typeof(ScriptableObject).IsAssignableFrom(type))
            {
                var reference = value as Dictionary<string, object>;
                object typeName, objectName;
                if (reference == null || !reference.TryGetValue("so.type", out typeName) || !(typeName is string)
                    || !reference.TryGetValue("so.name", out objectName) || !(objectName is string))
                    throw new FormatException("DataTable SO 引用必须含 so.type 与 so.name：" + path);
                Type actual = FindType((string)typeName);
                if (!type.IsAssignableFrom(actual)) throw new FormatException("DataTable SO 引用类型不匹配：" + path);
                return Resolve(actual, (string)objectName);
            }
            if (type == typeof(Sprite))
            {
                if (!(value is string)) throw new FormatException("DataTable Sprite 必须是旧资源路径：" + path);
                // Original SpriteConverter retains the existing sprite when the export
                // references an image absent from the selected MOD directory.
                return LegacyResourceRuntime.ResolveSprite(_package, (string)value) ?? previous;
            }
            if (type == typeof(AnimationCurve))
            {
                var rows = value as List<object>;
                if (rows == null) throw new FormatException("DataTable 曲线必须是关键帧数组：" + path);
                return new AnimationCurve(rows.Select(row => (Keyframe)ConvertValue(row, typeof(Keyframe), null, path)).ToArray());
            }
            if (type.IsEnum) return value is string ? Enum.Parse(type, (string)value, false)
                : Enum.ToObject(type, Convert.ToInt32(value, CultureInfo.InvariantCulture));
            if (type == typeof(string))
            {
                if (!(value is string)) throw new FormatException("DataTable 文本类型错误：" + path);
                return value;
            }
            if (type.IsPrimitive || type == typeof(decimal)) return Convert.ChangeType(value, type, CultureInfo.InvariantCulture);
            var array = value as List<object>;
            if (type.IsArray && array != null)
            {
                Type element = type.GetElementType();
                Array result = Array.CreateInstance(element, array.Count);
                for (int index = 0; index < array.Count; index++) result.SetValue(ConvertValue(array[index], element, null, path + "/" + index), index);
                return result;
            }
            if (typeof(IList).IsAssignableFrom(type) && array != null && type.IsGenericType)
            {
                var result = (IList)Activator.CreateInstance(type);
                foreach (object entry in array) result.Add(ConvertValue(entry, type.GetGenericArguments()[0], null, path));
                return result;
            }
            var obj = value as Dictionary<string, object>;
            if (typeof(IDictionary).IsAssignableFrom(type) && type.IsGenericType && obj != null)
            {
                var dictionary = (IDictionary)Activator.CreateInstance(type);
                Type[] arguments = type.GetGenericArguments();
                foreach (var pair in obj)
                    dictionary.Add(ConvertValue(pair.Key, arguments[0], null, path), ConvertValue(pair.Value, arguments[1], null, path + "/" + pair.Key));
                return dictionary;
            }
            if (obj == null || typeof(UObject).IsAssignableFrom(type)) throw new FormatException("DataTable 不支持字段类型：" + path + " (" + type.FullName + ")");
            object instance = Activator.CreateInstance(type, true);
            // Keyframe's JSON converter uses writable public properties. Other data
            // classes use exact fields, including private serialized StatValueReference.
            if (type == typeof(Keyframe))
            {
                foreach (var pair in obj)
                {
                    PropertyInfo property = type.GetProperty(pair.Key, BindingFlags.Public | BindingFlags.Instance);
                    if (property == null || !property.CanWrite) throw new FormatException("DataTable 关键帧属性不存在：" + path + "/" + pair.Key);
                    property.SetValue(instance, ConvertValue(pair.Value, property.PropertyType, null, path), null);
                }
            }
            else Fill(instance, obj, path);
            return instance;
        }

        private static FieldInfo[] Fields(Type type)
        {
            FieldInfo[] fields;
            if (FieldCache.TryGetValue(type, out fields)) return fields;
            var result = new List<FieldInfo>();
            for (Type current = type; current != null && current != typeof(ScriptableObject)
                && current != typeof(MonoBehaviour) && current != typeof(UObject); current = current.BaseType)
                result.AddRange(current.GetFields(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.DeclaredOnly)
                    .Where(field => !field.IsStatic));
            fields = result.ToArray(); FieldCache.Add(type, fields); return fields;
        }
        private static FieldInfo OptionalField(Type type, string name)
        {
            for (Type current = type; current != null; current = current.BaseType)
            {
                FieldInfo field = current.GetField(name, BindingFlags.Public | BindingFlags.NonPublic
                    | BindingFlags.Static | BindingFlags.Instance | BindingFlags.DeclaredOnly);
                if (field != null) return field;
            }
            return null;
        }
        private static IEnumerable<UObject> References(object source)
        {
            var result = new HashSet<UObject>();
            var seen = new HashSet<object>(ReferenceComparer.Instance);
            foreach (var field in Fields(source.GetType())) FindReferences(field.GetValue(source), result, seen);
            return result;
        }
        private static void FindReferences(object value, HashSet<UObject> result, HashSet<object> seen)
        {
            if (value == null || value is string || !seen.Add(value)) return;
            var so = value as ScriptableObject;
            if (so != null) { result.Add(so); return; }
            if (value is UObject) return;
            var dictionary = value as IDictionary;
            if (dictionary != null)
            {
                foreach (DictionaryEntry entry in dictionary)
                {
                    FindReferences(entry.Key, result, seen);
                    FindReferences(entry.Value, result, seen);
                }
                return;
            }
            var entries = value as IEnumerable;
            if (entries != null)
            {
                foreach (object entry in entries) FindReferences(entry, result, seen);
                return;
            }
            if (!Assemblies.Contains(value.GetType().Assembly)) return;
            foreach (var field in Fields(value.GetType())) FindReferences(field.GetValue(value), result, seen);
        }
        private static void RemapFields(object owner, bool record)
        {
            foreach (var field in Fields(owner.GetType()))
            {
                object original = field.GetValue(owner);
                object replacement = Remap(original, new Dictionary<object, object>(ReferenceComparer.Instance));
                if (ReferenceEquals(original, replacement)) continue;
                if (record) Replace(owner, field, replacement);
                else field.SetValue(owner, replacement);
            }
        }
        private static object Remap(object value, Dictionary<object, object> seen, bool detach = false)
        {
            if (value == null || value is string) return value;
            var unity = value as UObject;
            if (value is UObject)
            {
                ScriptableObject clone;
                return Copies.TryGetValue(unity, out clone) ? clone : value;
            }
            object known;
            if (seen.TryGetValue(value, out known)) return known;
            seen.Add(value, value);
            var dictionary = value as IDictionary;
            if (dictionary != null)
            {
                IDictionary detached = detach ? NewDictionary(value) : null;
                if (detached != null) seen[value] = detached;
                var mapped = new List<KeyValuePair<object, object>>();
                bool changed = detach;
                foreach (DictionaryEntry entry in dictionary)
                {
                    object mappedKey = Remap(entry.Key, seen, detach), mappedValue = Remap(entry.Value, seen, detach);
                    mapped.Add(new KeyValuePair<object, object>(mappedKey, mappedValue));
                    changed |= !ReferenceEquals(mappedKey, entry.Key) || !ReferenceEquals(mappedValue, entry.Value);
                }
                if (!changed) return value;
                var result = detached ?? NewDictionary(value);
                foreach (var entry in mapped) result.Add(entry.Key, entry.Value);
                seen[value] = result;
                return result;
            }
            var list = value as IList;
            if (list != null)
            {
                IList detached = detach ? (value is Array ? (IList)((Array)value).Clone() : (IList)Activator.CreateInstance(value.GetType())) : null;
                if (detached != null) seen[value] = detached;
                object[] mapped = new object[list.Count];
                bool changed = detach;
                for (int index = 0; index < list.Count; index++)
                {
                    object original = list[index];
                    mapped[index] = Remap(original, seen, detach);
                    changed |= !ReferenceEquals(mapped[index], original);
                }
                if (!changed) return value;
                var result = detached ?? (value is Array ? (IList)((Array)value).Clone() : (IList)Activator.CreateInstance(value.GetType()));
                if (!(value is Array)) foreach (var entry in mapped) result.Add(entry);
                else for (int index = 0; index < mapped.Length; index++) result[index] = mapped[index];
                seen[value] = result; return result;
            }
            var collection = value.GetType().GetInterfaces().FirstOrDefault(type => type.IsGenericType
                && type.GetGenericTypeDefinition() == typeof(ICollection<>));
            if (collection != null)
            {
                object detached = detach ? NewCollection(value) : null;
                if (detached != null) seen[value] = detached;
                var mapped = new List<object>();
                bool changed = detach;
                foreach (object original in (IEnumerable)value)
                {
                    object replacement = Remap(original, seen, detach);
                    mapped.Add(replacement);
                    changed |= !ReferenceEquals(original, replacement);
                }
                if (!changed) return value;
                object result = detached ?? NewCollection(value);
                MethodInfo add = collection.GetMethod("Add");
                foreach (object entry in mapped) add.Invoke(result, new[] { entry });
                seen[value] = result;
                return result;
            }
            if (!Assemblies.Contains(value.GetType().Assembly)) return value;
            object copy = detach ? typeof(object).GetMethod("MemberwiseClone", BindingFlags.NonPublic | BindingFlags.Instance).Invoke(value, null) : null;
            if (copy != null) seen[value] = copy;
            foreach (var field in Fields(value.GetType()))
            {
                object original = field.GetValue(value);
                object replacement = Remap(original, seen, detach);
                if (ReferenceEquals(original, replacement)) continue;
                if (copy == null) copy = typeof(object).GetMethod("MemberwiseClone", BindingFlags.NonPublic | BindingFlags.Instance).Invoke(value, null);
                field.SetValue(copy, replacement);
            }
            if (copy == null) return value;
            seen[value] = copy; return copy;
        }
        private static object NewCollection(object source)
        {
            PropertyInfo comparer = source.GetType().GetProperty("Comparer");
            var constructor = comparer == null ? null : source.GetType().GetConstructor(new[] { comparer.PropertyType });
            return constructor == null ? Activator.CreateInstance(source.GetType())
                : constructor.Invoke(new[] { comparer.GetValue(source, null) });
        }
        private static IDictionary NewDictionary(object source)
        {
            return (IDictionary)NewCollection(source);
        }
        private static void Replace(object owner, FieldInfo field, object value)
        {
            object original = field.GetValue(owner);
            field.SetValue(owner, value);
            Changes.Add(new ReferenceChange { Owner = owner, Field = field, Original = original, Replacement = value });
        }
        internal static T SessionObject<T>(T source) where T : UObject
        {
            ScriptableObject copy;
            return source != null && Copies.TryGetValue(source, out copy) ? copy as T : source;
        }
        private static Delegate EmptyHandler(Type type)
        {
            Delegate handler;
            if (EmptyHandlers.TryGetValue(type, out handler)) return handler;
            MethodInfo invoke = type.GetMethod("Invoke");
            var arguments = invoke.GetParameters().Select(parameter => Expression.Parameter(parameter.ParameterType)).ToArray();
            Expression body = invoke.ReturnType == typeof(void) ? (Expression)Expression.Empty() : Expression.Default(invoke.ReturnType);
            handler = Expression.Lambda(type, body, arguments).Compile();
            EmptyHandlers.Add(type, handler);
            return handler;
        }
        private static void MoveListeners()
        {
            foreach (var pair in Copies)
            {
                var source = pair.Key as ScriptableObject;
                if (source == null || Owned.Contains(source)) continue;
                foreach (var evt in source.GetType().GetEvents(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic))
                {
                    FieldInfo field = OptionalField(source.GetType(), evt.Name);
                    if (field == null || !typeof(Delegate).IsAssignableFrom(field.FieldType)) continue;
                    var binding = Events.FirstOrDefault(entry => ReferenceEquals(entry.Source, source) && entry.Field == field);
                    if (binding == null)
                    {
                        binding = new EventBinding { Source = source, Copy = pair.Value, Field = field };
                        Events.Add(binding);
                    }
                    Delegate empty = EmptyHandler(field.FieldType);
                    Delegate listeners = field.GetValue(source) as Delegate;
                    Delegate current = field.GetValue(binding.Copy) as Delegate;
                    foreach (Delegate listener in listeners == null ? new Delegate[0] : listeners.GetInvocationList())
                    {
                        if (listener.Equals(empty)) continue;
                        Delegate sessionListener = listener;
                        var owner = listener.Target as UObject;
                        ScriptableObject sessionOwner;
                        if (owner != null && Copies.TryGetValue(owner, out sessionOwner))
                            sessionListener = Delegate.CreateDelegate(listener.GetType(), sessionOwner, listener.Method);
                        if (current == null || !current.GetInvocationList().Contains(sessionListener)) current = Delegate.Combine(current, sessionListener);
                        binding.Moved[listener] = sessionListener;
                    }
                    field.SetValue(binding.Copy, current ?? empty);
                    // Only transient event subscriptions move. Serialized source data
                    // remains untouched and subscriptions follow the instance fields.
                    field.SetValue(source, empty);
                }
            }
        }
        private static void RestoreListeners()
        {
            var restored = new HashSet<string>(StringComparer.Ordinal);
            foreach (var binding in Events)
            {
                if (binding.Source == null || binding.Copy == null) continue;
                Delegate current = binding.Field.GetValue(binding.Copy) as Delegate;
                Delegate source = binding.Field.GetValue(binding.Source) as Delegate;
                string eventKey = binding.Copy.GetInstanceID() + "/" + binding.Field.DeclaringType.FullName + "/" + binding.Field.Name;
                var listeners = current == null ? new Delegate[0] : current.GetInvocationList();
                foreach (Delegate listener in listeners)
                {
                    Delegate original = binding.Moved.FirstOrDefault(pair => pair.Value.Equals(listener)).Key;
                    bool acquired = !Events.Any(entry => ReferenceEquals(entry.Copy, binding.Copy) && entry.Field == binding.Field && entry.Moved.Values.Contains(listener));
                    if (original == null && !(acquired && !restored.Contains(eventKey))) continue;
                    if (original == null)
                    {
                        original = listener;
                        UObject owner = Copies.FirstOrDefault(pair => ReferenceEquals(pair.Value, listener.Target) && !Owned.Contains(pair.Key)).Key;
                        if (owner != null) original = Delegate.CreateDelegate(listener.GetType(), owner, listener.Method);
                    }
                    if (source == null || !source.GetInvocationList().Contains(original)) source = Delegate.Combine(source, original);
                }
                binding.Field.SetValue(binding.Source, source ?? EmptyHandler(binding.Field.FieldType));
                restored.Add(eventKey);
            }
            Events.Clear();
        }
        private sealed class ReferenceComparer : IEqualityComparer<object>
        {
            internal static readonly ReferenceComparer Instance = new ReferenceComparer();
            public new bool Equals(object left, object right) { return ReferenceEquals(left, right); }
            public int GetHashCode(object value) { return System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(value); }
        }
    }

    [HarmonyPatch(typeof(CustomGameplayRuntime), "EnsurePackage")]
    internal static class LegacyDataTableEnsurePatch
    {
        private static void Postfix(ModPackage package) { LegacyDataTableRuntime.EnsurePackage(package); }
    }
    [HarmonyPatch(typeof(CustomGameplayRuntime), "Clear")]
    internal static class LegacyDataTableClearPatch
    {
        private static void Prefix() { LegacyDataTableRuntime.Clear(); }
    }
    [HarmonyPatch(typeof(CustomGameplayRuntime), "SessionItem")]
    internal static class LegacySessionItemPatch
    {
        private static void Postfix(ref ItemData __result) { __result = LegacyDataTableRuntime.SessionObject(__result); }
    }
    [HarmonyPatch(typeof(LuaManagerPatch), "StopActiveDevelopmentPlayback")]
    internal static class LegacyDevelopmentDataClearPatch
    {
        private static void Postfix() { LegacyDataTableRuntime.ClearDevelopment(); }
    }
}
