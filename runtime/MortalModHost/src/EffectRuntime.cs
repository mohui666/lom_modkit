using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // New controls own fresh non-unique instances. Native unique caches and
    // effects created outside this bridge are never adopted or modified.
    internal static class EffectRuntime
    {
        private sealed class ParticleSpeed
        {
            internal ParticleSystem System;
            internal float Original;
        }

        private sealed class Instance
        {
            internal StoryEffectsController Controller;
            internal GameObject Root;
            internal ParticleSystem System;
            internal Quaternion OriginalRotation;
            internal float OriginalScaleZ;
            internal ParticleSpeed[] Speeds;
            internal EffectPlaybackState Playback;
            internal Coroutine Timer;
        }

        private static readonly FieldInfo NonUniqueInstances =
            AccessTools.Field(typeof(StoryEffectsController), "_nonUniqueInstances");
        private static readonly List<Instance> Instances = new List<Instance>();
        private static MonoBehaviour _host;
        private static ModPackage _owner;
        private static StoryEffectsController _controller;

        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static void Run(StoryEffectsController controller, string name,
            float x, float y, float scaleX, float scaleY, float unique, bool play,
            string space, float rotation, float speed, float stopAfter, bool clear,
            bool restart, string tag)
        {
            if (controller == null) throw new InvalidOperationException("当前 Lua 环境没有原版特效控制器");
            if (ModOverlay.CurrentPackage == null) throw new InvalidOperationException("特效控制需要当前 MOD 会话");
            if (string.IsNullOrEmpty(name) && (play || string.IsNullOrEmpty(tag)))
                throw new ArgumentException("特效名称不能为空");
            if (!Finite(x) || !Finite(y) || !Finite(scaleX) || !Finite(scaleY) || !Finite(unique)
                || !Finite(rotation) || !Finite(speed) || speed < 0f || !Finite(stopAfter)
                || (stopAfter != -1f && stopAfter < 0f))
                throw new ArgumentOutOfRangeException("effect", "特效参数须为有限数字，速度非负，定时为 -1 或非负秒数");
            if (space != "world" && space != "screen")
                throw new ArgumentException("特效坐标必须使用 world 或 screen");
            if (!ReferenceEquals(_owner, ModOverlay.CurrentPackage) || _controller != controller)
            {
                Clear();
                _owner = ModOverlay.CurrentPackage;
                _controller = controller;
            }
            if (!play)
            {
                foreach (Instance item in Instances)
                    if (item.Playback.MatchesStop(name, tag)) Stop(item, clear);
                return;
            }

            Instance instance = unique == 1f
                ? Instances.Find(item => item.Root != null && item.Playback.CanReuse(name, tag)) : null;
            if (instance != null && instance.Playback.Name != name)
            {
                Destroy(instance);
                Instances.Remove(instance);
                instance = null;
            }
            bool created = instance == null;
            if (created)
            {
                // SetupEffect synchronously waits for Addressables before its
                // Instantiate, so exactly this appended entry is our instance.
                instance = Create(controller, name, space == "screen" ? 0f : x,
                    space == "screen" ? 0f : y, scaleX, scaleY, unique == 1f, tag);
                Instances.Add(instance);
            }
            long generation = CancelTimer(instance);
            RenderPriorityRuntime.TrackEffect(instance.Root, instance.Playback.Name, instance.Playback.Tag);
            Transform transform = instance.Root.transform;
            transform.position = space == "screen" ? ScreenPosition(instance, x, y) : new Vector3(x, y, 0f);
            transform.localScale = new Vector3(scaleX, scaleY, instance.OriginalScaleZ);
            transform.localRotation = instance.OriginalRotation * Quaternion.Euler(0f, 0f, rotation);
            foreach (ParticleSpeed item in instance.Speeds)
            {
                if (item.System == null) continue;
                float value = item.Original * speed;
                if (!Finite(value)) throw new ArgumentOutOfRangeException("speed", "特效速度倍率超出粒子系统数值范围");
                ParticleSystem.MainModule main = item.System.main;
                main.simulationSpeed = value;
            }
            if (restart) instance.System.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
            if (!created || restart) instance.System.Play();
            if (stopAfter == 0f) Stop(instance, clear);
            else if (stopAfter > 0f)
            {
                if (_host == null) throw new InvalidOperationException("特效计时宿主尚未初始化");
                instance.Timer = _host.StartCoroutine(StopAfter(instance, generation, stopAfter, clear));
            }
        }

        private static bool Finite(float value) { return !float.IsNaN(value) && !float.IsInfinity(value); }

        private static List<GameObject> NativeInstances(StoryEffectsController controller)
        {
            if (NonUniqueInstances == null)
                throw new InvalidOperationException("原版缺少 StoryEffectsController._nonUniqueInstances");
            var list = NonUniqueInstances.GetValue(controller) as List<GameObject>;
            if (list == null) throw new InvalidOperationException("原版特效控制器尚未初始化实例列表");
            return list;
        }

        private static Instance Create(StoryEffectsController controller, string name,
            float x, float y, float scaleX, float scaleY, bool reusable, string tag)
        {
            List<GameObject> list = NativeInstances(controller);
            int before = list.Count;
            controller.SetupEffect(name, x, y, scaleX, scaleY, 0, 1);
            if (list.Count != before + 1 || list[before] == null)
                throw new InvalidOperationException("原版没有创建预期的特效实例：" + name);
            GameObject root = list[before];
            ParticleSystem system = root.GetComponent<ParticleSystem>();
            if (system == null) throw new InvalidOperationException("原版特效根物件没有 ParticleSystem：" + name);
            ParticleSystem[] systems = root.GetComponentsInChildren<ParticleSystem>(true);
            var speeds = new ParticleSpeed[systems.Length];
            for (int i = 0; i < systems.Length; i++)
                speeds[i] = new ParticleSpeed { System = systems[i], Original = systems[i].main.simulationSpeed };
            return new Instance
            {
                Controller = controller, Root = root, System = system,
                OriginalRotation = root.transform.localRotation, OriginalScaleZ = root.transform.localScale.z,
                Speeds = speeds, Playback = new EffectPlaybackState(name, tag, reusable)
            };
        }

        private static Vector3 ScreenPosition(Instance instance, float x, float y)
        {
            int layers = 0;
            foreach (ParticleSystemRenderer renderer in instance.Root.GetComponentsInChildren<ParticleSystemRenderer>(true))
                if (renderer.enabled && renderer.gameObject.activeInHierarchy) layers |= 1 << renderer.gameObject.layer;
            Camera camera = null;
            foreach (Camera candidate in Resources.FindObjectsOfTypeAll<Camera>())
            {
                if (candidate == null || !candidate.isActiveAndEnabled || candidate.gameObject.name != "StageCamera"
                    || candidate.gameObject.scene != instance.Controller.gameObject.scene
                    || (candidate.cullingMask & layers) == 0) continue;
                if (camera != null) throw new InvalidOperationException("当前特效场景存在多个可用 StageCamera");
                camera = candidate;
            }
            if (camera == null) throw new InvalidOperationException("找不到可渲染本特效的当前 StageCamera，无法使用画面百分比坐标");
            Ray ray = camera.ViewportPointToRay(new Vector3(x / 100f, 1f - y / 100f, 0f));
            float distance;
            if (!new Plane(Vector3.forward, Vector3.zero).Raycast(ray, out distance))
                throw new InvalidOperationException("当前 StageCamera 射线未与特效 z=0 平面相交");
            return ray.GetPoint(distance);
        }

        private static long CancelTimer(Instance instance)
        {
            if (instance.Timer != null && _host != null) _host.StopCoroutine(instance.Timer);
            instance.Timer = null;
            return instance.Playback.NextPlayback();
        }

        private static void Stop(Instance instance, bool clear)
        {
            CancelTimer(instance);
            if (instance.System != null)
                instance.System.Stop(true, clear ? ParticleSystemStopBehavior.StopEmittingAndClear : ParticleSystemStopBehavior.StopEmitting);
        }

        private static IEnumerator StopAfter(Instance instance, long generation, float seconds, bool clear)
        {
            float elapsed = 0f;
            while (elapsed < seconds)
            {
                if (instance.Root == null || !instance.Playback.IsCurrent(generation)) yield break;
                elapsed += Time.deltaTime;
                yield return null;
            }
            if (instance.Playback.IsCurrent(generation))
            {
                instance.Timer = null;
                Stop(instance, clear);
            }
        }

        private static void Destroy(Instance instance)
        {
            Stop(instance, true);
            if (instance.Controller != null && !ReferenceEquals(instance.Root, null))
                NativeInstances(instance.Controller).Remove(instance.Root);
            if (instance.Root != null)
            {
                instance.Root.SetActive(false);
                UnityEngine.Object.Destroy(instance.Root);
            }
        }

        internal static void RegisterPriorities()
        {
            foreach (Instance instance in Instances)
                RenderPriorityRuntime.TrackEffect(instance.Root, instance.Playback.Name, instance.Playback.Tag);
        }

        internal static void Clear()
        {
            foreach (Instance instance in Instances) Destroy(instance);
            Instances.Clear();
            _owner = null;
            _controller = null;
        }
    }
}
