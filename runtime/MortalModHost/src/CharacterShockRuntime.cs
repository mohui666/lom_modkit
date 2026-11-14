using System;
using System.Collections;
using System.Collections.Generic;
using Fungus;
using HarmonyLib;
using Mortal.Story;
using UnityEngine;

namespace MortalModHost
{
    // Reuse the loaded game's shock settings without writing its shared
    // ShockPosition/Duration variables or invoking the shared block callback.
    internal static class CharacterShockRuntime
    {
        private sealed class Motion
        {
            internal Character Character;
            internal RectTransform Holder;
            internal Vector3 Origin;
            internal bool Local;
            internal string Name;
            internal iTween Tween;
            internal Coroutine Watcher;
        }

        private static MonoBehaviour _host;
        private static readonly Dictionary<Character, Motion> Motions = new Dictionary<Character, Motion>();

        internal static void Init(MonoBehaviour host) { _host = host; }

        internal static bool Shock(string id, float duration)
        {
            if (float.IsNaN(duration) || float.IsInfinity(duration) || duration < 0f)
                throw new ArgumentOutOfRangeException("duration", "震动时长必须为非负有限秒数");
            CharacterPlaceholder characters = CharacterPlaceholder.Instance;
            Character character = characters != null ? characters.Get(id) : null;
            if (character == null || character.State == null || character.State.holder == null || !character.State.onScreen)
                throw new InvalidOperationException("震动人物尚未登场：" + id);
            // A zero-time stage.show may still have a pending MOVE tween.
            // Give this shake ownership of the holder before sampling origin.
            CharacterMovementRuntime.CancelStageMovement(character.State.holder);
            if (duration == 0f) return true;
            if (_host == null) throw new InvalidOperationException("人物震动宿主尚未初始化");

            ShakePosition source = FindTemplate(characters);
            bool local = Read<bool>(source, typeof(ShakePosition), "isLocal");
            var motion = new Motion
            {
                Character = character,
                Holder = character.State.holder,
                Local = local,
                Name = "MortalModHost.Shock." + character.GetInstanceID(),
            };
            motion.Origin = local ? motion.Holder.localPosition : motion.Holder.position;
            var args = new Hashtable
            {
                { "name", motion.Name },
                { "amount", Read<Vector3Data>(source, typeof(ShakePosition), "_amount").Value },
                { "time", duration },
                { "islocal", local },
                { "easetype", Read<iTween.EaseType>(source, typeof(iTweenCommand), "easeType") },
                { "looptype", Read<iTween.LoopType>(source, typeof(iTweenCommand), "loopType") },
            };
            switch (Read<iTweenAxis>(source, typeof(ShakePosition), "axis"))
            {
                case iTweenAxis.X: args.Add("axis", "x"); break;
                case iTweenAxis.Y: args.Add("axis", "y"); break;
                case iTweenAxis.Z: args.Add("axis", "z"); break;
            }
            // The same native iTween implementation retains its amount,
            // decay, axis and default game-time clock for each target.
            iTween.ShakePosition(motion.Holder.gameObject, args);
            foreach (iTween tween in motion.Holder.GetComponents<iTween>())
                if (tween != null && tween._name == motion.Name) motion.Tween = tween;
            if (motion.Tween == null) throw new InvalidOperationException("原版人物震动未创建：" + id);
            Motions.Add(character, motion);
            motion.Watcher = _host.StartCoroutine(Observe(motion));
            return true;
        }

        private static T Read<T>(object source, Type owner, string name)
        {
            var field = AccessTools.Field(owner, name);
            if (field == null) throw new MissingFieldException(owner.FullName, name);
            return (T)field.GetValue(source);
        }

        private static ShakePosition FindTemplate(CharacterPlaceholder characters)
        {
            ShakePosition selected = null;
            foreach (FlowchartPlaceholder placeholder in UnityEngine.Object.FindObjectsOfType<FlowchartPlaceholder>())
            {
                if (placeholder.gameObject.scene != characters.gameObject.scene || placeholder.common == null) continue;
                Block block = placeholder.common.FindBlock("shock");
                if (block == null) continue;
                foreach (Command command in block.CommandList)
                {
                    var shake = command as ShakePosition;
                    if (shake == null) continue;
                    if (selected != null) throw new InvalidOperationException("当前剧情包含多个原版震动配置，无法安全选择");
                    selected = shake;
                }
            }
            if (selected == null) throw new InvalidOperationException("当前剧情缺少 common/shock 的原版震动配置");
            return selected;
        }

        private static IEnumerator Observe(Motion motion)
        {
            while (motion.Tween != null && motion.Holder != null && motion.Character != null
                && motion.Character.State != null && motion.Character.State.holder == motion.Holder
                && motion.Character.State.onScreen)
                yield return null;
            // Native completion already restores its origin. Do not write it
            // again after a following step may have moved the same character.
            if (motion.Tween != null) Stop(motion);
            Motions.Remove(motion.Character);
            motion.Watcher = null;
        }

        private static void Stop(Motion motion)
        {
            if (motion.Tween == null) return;
            // Dispose uses deferred Destroy; prevent a later Update in this
            // frame from overwriting the action which cancelled this shock.
            motion.Tween.enabled = false;
            motion.Tween.isRunning = false;
            if (motion.Holder != null)
            {
                iTween.StopByName(motion.Holder.gameObject, motion.Name);
                if (motion.Local) motion.Holder.localPosition = motion.Origin;
                else motion.Holder.position = motion.Origin;
            }
            motion.Tween = null;
        }

        internal static void Forget(Transform holder)
        {
            if (ReferenceEquals(holder, null)) return;
            Motion found = null;
            foreach (Motion motion in Motions.Values)
                if (ReferenceEquals(motion.Holder, holder)) { found = motion; break; }
            if (found == null) return;
            if (found.Watcher != null && _host != null) _host.StopCoroutine(found.Watcher);
            found.Watcher = null;
            Stop(found);
            Motions.Remove(found.Character);
        }

        internal static void Clear()
        {
            foreach (Motion motion in Motions.Values)
            {
                if (motion.Watcher != null && _host != null) _host.StopCoroutine(motion.Watcher);
                Stop(motion);
            }
            Motions.Clear();
        }
    }

    [HarmonyPatch(typeof(PortraitController), "SetupPortrait", new[] { typeof(PortraitOptions) })]
    internal static class CharacterShockSetupPatch
    {
        private static void Prefix(PortraitOptions options)
        {
            if (options.character != null && options.character.State != null)
                CharacterShockRuntime.Forget(options.character.State.holder);
        }
    }
}
