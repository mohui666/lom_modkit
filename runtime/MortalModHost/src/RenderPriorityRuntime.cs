using System;
using System.Collections.Generic;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Only runtime instances on the Story stage participate. UI cameras and
    // official particle assets are never moved or modified.
    internal static class RenderPriorityRuntime
    {
        private sealed class Rule
        {
            internal string Target, Key, Tag;
            internal int Order;
            internal bool Enabled;
        }
        private sealed class Particle
        {
            internal Renderer Renderer;
            internal int Layer, Order, Offset;
            internal Material[] Original, Copies;
        }
        private sealed class Entry
        {
            internal GameObject Root;
            internal string Target, Key, Tag;
            internal Canvas Canvas;
            internal bool AddedCanvas, Override;
            internal int Layer, Order, Sibling, Priority, Sequence, GameLayer;
            internal Particle[] Particles;
        }
        private static readonly List<Rule> Rules = new List<Rule>();
        private static readonly List<Entry> Entries = new List<Entry>();
        private static Canvas _stage;
        private static bool _active, _dirty;
        private static int _sequence;

        internal static int Priority(string target, string key, string tag = "")
        {
            for (int i = Rules.Count - 1; i >= 0; i--)
            {
                Rule rule = Rules[i];
                if (rule.Target != target) continue;
                bool matches = target == "effect" && rule.Tag.Length > 0
                    ? rule.Tag == tag : rule.Key == key;
                if (matches) return rule.Enabled ? rule.Order : 0;
            }
            return 0;
        }

        internal static void Set(string target, string key, string tag, float order, bool enabled)
        {
            if (ModOverlay.CurrentPackage == null) throw new InvalidOperationException("渲染优先级只能用于 MOD 演出");
            if (target != "character" && target != "background" && target != "custom_cg" && target != "overlay" && target != "effect")
                throw new ArgumentException("未知排序对象：" + target);
            if (float.IsNaN(order) || float.IsInfinity(order) || order != Mathf.Floor(order) || Mathf.Abs(order) > 10000f)
                throw new ArgumentOutOfRangeException("order", "渲染优先级必须是 -10000～10000 的整数");
            key = key ?? ""; tag = target == "effect" ? tag ?? "" : "";
            if ((target == "character" || target == "overlay") && string.IsNullOrWhiteSpace(key)
                || target == "effect" && string.IsNullOrWhiteSpace(key) && string.IsNullOrWhiteSpace(tag))
                throw new ArgumentException("请填写排序目标人物、图片槽位或特效名称／标记");
            if (target == "effect" && tag.Length > 0) key = "";
            Rules.RemoveAll(rule => rule.Target == target && rule.Key == key && rule.Tag == tag);
            Rules.Add(new Rule { Target = target, Key = key, Tag = tag, Order = (int)order, Enabled = enabled });
            bool active = Rules.Exists(rule => rule.Enabled);
            if (active && !_active)
            {
                Stage stage = Stage.GetActiveStage();
                if (stage == null || stage.PortraitCanvas == null) throw new InvalidOperationException("当前没有可排序的剧情舞台");
                _stage = stage.PortraitCanvas;
                _active = true;
                Canvas.willRenderCanvases += Refresh;
                // Include everything already visible before the first sorting step.
                if (stage.CharactersOnStage != null)
                    foreach (Character character in stage.CharactersOnStage) Character(character as StoryCharacterController);
                foreach (string id in CustomCharacterRuntime.ActiveCharacterIds())
                    TrackUi(CustomCharacterRuntime.FilterHolder(id), "character", id);
                CustomImageRuntime.RegisterPriorities();
                EffectRuntime.RegisterPriorities();
            }
            else if (!active && _active)
            {
                _active = false;
                Canvas.willRenderCanvases -= Refresh;
                foreach (Entry entry in Entries) Restore(entry);
            }
            _dirty = true;
            Refresh();
        }

        internal static void Character(StoryCharacterController character)
        {
            if (_active && character != null && character.Data != null && character.State != null && character.State.holder != null)
                TrackUi(character.State.holder.gameObject, "character", character.Data.Id);
        }

        internal static void TrackUi(GameObject root, string target, string key = "")
        {
            if (!_active || root == null) return;
            Entry old = Entries.Find(entry => entry.Root == root);
            if (old != null) { _dirty = true; return; }
            Canvas canvas = root.GetComponent<Canvas>();
            bool added = canvas == null;
            if (added) canvas = root.AddComponent<Canvas>();
            Entries.Add(new Entry { Root = root, Target = target, Key = key, Tag = "", Canvas = canvas,
                AddedCanvas = added, Override = canvas.overrideSorting, Layer = canvas.sortingLayerID,
                Order = canvas.sortingOrder, GameLayer = root.layer, Sibling = root.transform.GetSiblingIndex(), Sequence = _sequence++ });
            _dirty = true;
        }

        internal static void TrackEffect(GameObject root, string name, string tag)
        {
            if (!_active || root == null) return;
            if (Entries.Exists(entry => entry.Root == root)) return;
            Renderer[] renderers = root.GetComponentsInChildren<Renderer>(true);
            var particles = new Particle[renderers.Length];
            for (int i = 0; i < renderers.Length; i++)
                particles[i] = new Particle { Renderer = renderers[i], Layer = renderers[i].sortingLayerID,
                    Order = renderers[i].sortingOrder, Original = renderers[i].sharedMaterials };
            var ordered = (Particle[])particles.Clone();
            Array.Sort(ordered, (a,b) => {
                int c = SortingLayer.GetLayerValueFromID(a.Layer).CompareTo(SortingLayer.GetLayerValueFromID(b.Layer));
                return c != 0 ? c : a.Order.CompareTo(b.Order);
            });
            int offset = 0;
            for (int i = 0; i < ordered.Length; i++)
            {
                if (i > 0 && (ordered[i].Layer != ordered[i-1].Layer || ordered[i].Order != ordered[i-1].Order)) offset++;
                ordered[i].Offset = offset;
            }
            Entries.Add(new Entry { Root = root, Target = "effect", Key = name, Tag = tag,
                Particles = particles, Sequence = _sequence++ });
            _dirty = true;
        }

        private static void Refresh()
        {
            if (!_active || _stage == null) return;
            for (int i = Entries.Count - 1; i >= 0; i--)
            {
                Entry entry = Entries[i];
                if (entry.Root == null) { Restore(entry); Entries.RemoveAt(i); _dirty = true; continue; }
                int sibling = entry.Root.transform.GetSiblingIndex();
                int priority = Priority(entry.Target, entry.Key, entry.Tag);
                if (sibling != entry.Sibling || priority != entry.Priority) _dirty = true;
                entry.Sibling = sibling; entry.Priority = priority;
            }
            if (!_dirty) return;
            Entries.Sort((a, b) => {
                int c = a.Priority.CompareTo(b.Priority);
                if (c != 0) return c;
                // Native stage UI follows sibling/focus order. Effects retain
                // their native sorting order relative to the stage canvas.
                int an = a.Canvas != null ? a.Sibling : NaturalEffectOrder(a);
                int bn = b.Canvas != null ? b.Sibling : NaturalEffectOrder(b);
                c = an.CompareTo(bn);
                return c != 0 ? c : a.Sequence.CompareTo(b.Sequence);
            });
            int rank = _stage.sortingOrder + 1;
            foreach (Entry entry in Entries)
            {
                if (entry.Canvas != null)
                {
                    if (rank >= 32767) throw new InvalidOperationException("舞台排序对象过多，超出 Unity 排序范围");
                    entry.Canvas.overrideSorting = true;
                    entry.Root.layer = _stage.gameObject.layer;
                    entry.Canvas.sortingLayerID = _stage.sortingLayerID;
                    entry.Canvas.sortingOrder = rank++;
                }
                else
                {
                    int max = 0;
                    foreach (Particle particle in entry.Particles) max = Math.Max(max, particle.Offset);
                    if ((long)rank + max >= 32767) throw new InvalidOperationException("特效内部排序跨度超出 Unity 排序范围");
                    foreach (Particle particle in entry.Particles)
                    {
                        if (particle.Renderer == null) continue;
                        particle.Renderer.sortingLayerID = _stage.sortingLayerID;
                        particle.Renderer.sortingOrder = rank + particle.Offset;
                        // Transparent UI and particle shaders must share a
                        // queue before sortingOrder can interleave them.
                        if (particle.Copies == null && Array.Exists(particle.Original,material => material != null && material.renderQueue != 3000))
                        {
                            particle.Copies = new Material[particle.Original.Length];
                            for (int i = 0; i < particle.Original.Length; i++)
                                if (particle.Original[i] != null)
                                {
                                    particle.Copies[i] = particle.Original[i].renderQueue == 3000 ? particle.Original[i] : new Material(particle.Original[i]);
                                    if (particle.Copies[i] != particle.Original[i]) particle.Copies[i].renderQueue = 3000;
                                }
                            particle.Renderer.sharedMaterials = particle.Copies;
                        }
                    }
                    rank += max + 1;
                }
            }
            _dirty = false;
        }

        private static int NaturalEffectOrder(Entry entry)
        {
            int stageLayer = SortingLayer.GetLayerValueFromID(_stage.sortingLayerID);
            foreach (Particle particle in entry.Particles)
            {
                int layer = SortingLayer.GetLayerValueFromID(particle.Layer);
                if (layer > stageLayer || layer == stageLayer && particle.Order > _stage.sortingOrder
                    || Array.Exists(particle.Original, material => material != null && material.renderQueue > 3000))
                    return int.MaxValue;
            }
            return -1;
        }

        private static void Restore(Entry entry)
        {
            if (entry.Canvas != null)
            {
                // An added Canvas still needs the stage camera's layer even
                // with overrideSorting off. Keep it for this holder's lifetime:
                // clear/re-show in the same frame must not adopt a Canvas
                // that is waiting for Unity's deferred destruction.
                if (!entry.AddedCanvas) entry.Root.layer = entry.GameLayer;
                entry.Canvas.overrideSorting = entry.Override;
                entry.Canvas.sortingLayerID = entry.Layer;
                entry.Canvas.sortingOrder = entry.Order;
            }
            if (entry.Particles == null) return;
            foreach (Particle particle in entry.Particles)
            {
                if (particle.Renderer != null)
                {
                    particle.Renderer.sortingLayerID = particle.Layer;
                    particle.Renderer.sortingOrder = particle.Order;
                    particle.Renderer.sharedMaterials = particle.Original;
                }
                if (particle.Copies != null)
                    for (int i = 0; i < particle.Copies.Length; i++)
                        if (particle.Copies[i] != null && particle.Copies[i] != particle.Original[i]) UnityEngine.Object.Destroy(particle.Copies[i]);
                particle.Copies = null;
            }
        }

        internal static void Clear()
        {
            Canvas.willRenderCanvases -= Refresh;
            foreach (Entry entry in Entries)
            {
                Restore(entry);
            }
            Entries.Clear(); Rules.Clear(); _active = false; _dirty = false; _stage = null; _sequence = 0;
        }
    }

    [HarmonyPatch(typeof(PortraitController), "SetupPortrait", new[] { typeof(PortraitOptions) })]
    internal static class RenderPriorityPortraitPatch
    {
        private static void Postfix(PortraitOptions options) { RenderPriorityRuntime.Character(options.character as StoryCharacterController); }
    }
    [HarmonyPatch(typeof(StoryCharacterController), "Focus", new Type[0])]
    internal static class RenderPriorityFocusPatch
    {
        private static void Postfix(StoryCharacterController __instance) { RenderPriorityRuntime.Character(__instance); }
    }
}
