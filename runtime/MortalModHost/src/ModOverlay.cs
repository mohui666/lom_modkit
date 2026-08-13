using System;
using System.Collections;
using BepInEx.Logging;
using HarmonyLib;
using Mortal.Core;
using UnityEngine;
using UnityEngine.UI;

namespace MortalModHost
{
    /// <summary>
    /// mod 死亡/结局文本覆盖（契约 §C）：mod 用自造 id（9+官方id，如 910021）进官方 GameOver/End
    /// 场景时，LibrarySystem 查不到该 id，画面没有文本（死亡没文本、结局黑屏）。编译器在 mod Lua 里
    /// 发射裸全局调用 mod_set_death_text(text) / mod_set_ending_text(title, desc)，LuaManagerPatch 把
    /// 参数写进本类；随后进入 GameOver/End 场景时，GameOverOverlayPatch / EndGameOverlayPatch 用
    /// Harmony postfix 包一层官方 Start 协程，在官方"查表失败、清空文本"之后、"fade 序列开始之前"
    /// 把 mod 文本写进官方画面组件（Text/描述行 prefab），文本随官方 DOFade 渐显——布局 100% 官方。
    /// 官方脚本分支演出时 Clear()（官方结局不受影响）；场景切换离开 GameOver/End 时由 Plugin.Update 清除。
    /// </summary>
    internal static class ModOverlay
    {
        /// <summary>死亡文本（GameOver 画面中央显示）。</summary>
        internal static string DeathText;

        /// <summary>结局卡片标题（End 画面；GameOver 场景优先于死亡文本显示）。</summary>
        internal static string EndingTitle;

        /// <summary>结局卡片描述（可多行，中文按 \n 拆行）。</summary>
        internal static string EndingDesc;

        /// <summary>GameOver 画面是否有 mod 文本可画（结局优先，其次死亡文本）。</summary>
        internal static bool HasGameOverContent
        {
            get { return !string.IsNullOrEmpty(EndingTitle) || !string.IsNullOrEmpty(DeathText); }
        }

        /// <summary>End 画面是否有 mod 结局文本可画（只看结局标题）。</summary>
        internal static bool HasEndingContent
        {
            get { return !string.IsNullOrEmpty(EndingTitle); }
        }

        internal static void SetDeathText(string text)
        {
            DeathText = string.IsNullOrEmpty(text) ? null : text;
        }

        internal static void SetEnding(string title, string desc)
        {
            EndingTitle = string.IsNullOrEmpty(title) ? null : title;
            EndingDesc = string.IsNullOrEmpty(desc) ? null : desc;
        }

        internal static void Clear()
        {
            DeathText = null;
            EndingTitle = null;
            EndingDesc = null;
        }
    }

    /// <summary>
    /// Harmony patch：<c>GameOverController.Start()</c>（private IEnumerator，死亡画面文本设置与 fade 序列）。
    ///
    /// 反编译结论（ilspycmd 8.2，Mortal.Core.dll，与游戏当前版本一致）：
    /// <list type="number">
    /// <item>Start 是 IEnumerator：开头 Destroy _descContainer/_horizontalDescContainer 旧行、四个 DOFade(0,0) 归零、
    ///     停音乐，然后 <c>yield return SceneController.Instance.UnloadLoading()</c>。</item>
    /// <item>随后 <c>_titleText.text = ""</c> 并用 CurrentSceneKey 查 LibrarySystem.Dead——mod 自造 id 查不到，
    ///     文本保持空；接着 <c>yield return _canvasGroup.DOFade(1f, _fadeDuration).WaitForCompletion()</c>。</item>
    /// <item>画布 fade 后等 0.5s，再按 IsChineseLanguage 分两条路 fade：中文 <c>_titleText.DOFade(1,1)</c> +
    ///     <c>_descContainer.DOFade(1,1)</c>；非中文 <c>_horizontalTitleText.DOFade(1,1)</c> +
    ///     <c>_horizontalDescContainer.DOFade(1,1)</c>；最后按钮 fade。</item>
    /// </list>
    /// Harmony 的 postfix 在 IEnumerator 方法上运行于"枚举器对象刚创建、协程体尚未执行"时——此时设文本会被
    /// 官方步骤 2 的 <c>_titleText.text = ""</c> 清掉。所以这里把 <c>__result</c> 替换成一个包装协程：
    /// 手动步进官方枚举器到"查表之后、画布 fade 之前"注入文本，后续 yield 全部透传——文本恰好随官方
    /// 标题/描述 DOFade 渐显，不用引用 DOTween，也无需手工 alpha。
    /// </summary>
    [HarmonyPatch(typeof(GameOverController), "Start")]
    internal static class GameOverOverlayPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        private static void Postfix(GameOverController __instance, ref IEnumerator __result)
        {
            if (!ModOverlay.HasGameOverContent) return;
            try
            {
                IEnumerator original = __result;
                __result = ApplyAfterOfficial(__instance, original);
            }
            catch (Exception ex)
            {
                Log?.LogWarning("GameOver mod 文本覆盖注入失败：" + ex.Message);
            }
        }

        /// <summary>包装协程：步进官方 Start 到文本注入点，注入后透传剩余 yield。
        /// 注意迭代器限制：catch 全部收敛在 Step/ApplyGameOverText 普通方法里，
        /// 这里只有 finally（释放官方枚举器），意外异常与官方协程异常行为一致地抛给 Unity。
        /// </summary>
        private static IEnumerator ApplyAfterOfficial(GameOverController controller, IEnumerator original)
        {
            try
            {
                // 步进 1：官方执行到 UnloadLoading yield（旧描述行已 Destroy、fade 已归零、音乐已停）
                if (!Step(original)) yield break;
                yield return original.Current;
                // 步进 2：官方执行完 _titleText.text="" 与 LibrarySystem.Dead 查表（mod id 必不命中），停在画布 fade yield 上
                if (!Step(original)) yield break;
                // 在官方 fade 序列开始前注入 mod 文本：随后官方标题/描述 DOFade 会把我们的文本渐显出来
                ApplyGameOverText(controller);
                yield return original.Current;
                // 后续 yield 全部透传：画布 fade → 0.5s 等待 → 标题/描述 fade → 按钮 fade
                while (Step(original))
                    yield return original.Current;
            }
            finally
            {
                (original as IDisposable)?.Dispose();
            }
        }

        /// <summary>官方枚举器步进；异常按官方协程异常语义处理（日志后终止），不抛给 Unity。</summary>
        private static bool Step(IEnumerator enumerator)
        {
            try
            {
                return enumerator.MoveNext();
            }
            catch (Exception ex)
            {
                Log?.LogWarning("GameOver 官方 Start 步进异常（终止 mod 文本覆盖，与官方协程异常行为一致）：" + ex.Message);
                return false;
            }
        }

        /// <summary>
        /// 把 mod 文本写进官方组件：标题写 _titleText 与 _horizontalTitleText（官方两条路都设）；
        /// 描述按官方同款方式 Instantiate(_descTextPrefab) 到容器（与官方一致按 IsChineseLanguage
        /// 二选一：中文纵向容器拆 \n 多行，非中文横排容器整段一行）。
        /// </summary>
        private static void ApplyGameOverText(GameOverController controller)
        {
            try
            {
                var traverse = Traverse.Create(controller);
                Text title = traverse.Field("_titleText").GetValue<Text>();
                Text horizontalTitle = traverse.Field("_horizontalTitleText").GetValue<Text>();
                GameObject prefab = traverse.Field("_descTextPrefab").GetValue<GameObject>();
                CanvasGroup descContainer = traverse.Field("_descContainer").GetValue<CanvasGroup>();
                CanvasGroup horizontalDescContainer = traverse.Field("_horizontalDescContainer").GetValue<CanvasGroup>();

                // GameOver 优先级：结局 title/desc &gt; 死亡文本当标题（契约 §C）
                string titleText = !string.IsNullOrEmpty(ModOverlay.EndingTitle)
                    ? ModOverlay.EndingTitle
                    : ModOverlay.DeathText;
                if (title != null) title.text = titleText;
                if (horizontalTitle != null) horizontalTitle.text = titleText;

                string desc = ModOverlay.EndingDesc;
                if (string.IsNullOrEmpty(desc) || prefab == null) return;
                if (SystemSettings.IsChineseLanguage)
                {
                    if (descContainer != null)
                    {
                        string[] lines = desc.Split('\n');
                        foreach (string line in lines)
                            UnityEngine.Object.Instantiate(prefab, descContainer.transform, false).GetComponent<Text>().text = line;
                    }
                }
                else if (horizontalDescContainer != null)
                {
                    UnityEngine.Object.Instantiate(prefab, horizontalDescContainer.transform, false).GetComponent<Text>().text = desc;
                }
            }
            catch (Exception ex)
            {
                Log?.LogWarning("GameOver mod 文本写入失败：" + ex);
            }
        }
    }

    /// <summary>
    /// Harmony patch：<c>EndGameController.Start()</c>（private IEnumerator，结局画面）。
    ///
    /// 反编译结论（ilspycmd 8.2，Mortal.Core.dll，与游戏当前版本一致）：
    /// <list type="number">
    /// <item>Start 是 IEnumerator：开头 <c>_titleText.text=""; _descText.text="";</c> 两个 DOFade(0,0) 归零、
    ///     按钮画布 alpha 0，然后 <c>yield return UnloadLoading()</c>。</item>
    /// <item>等 0.5s 后按 CurrentSceneKey 查 LibrarySystem.EndGame——mod 自造 id 查不到 → 文本保持空；
    ///     官方只在查表命中时 fade 标题/描述（if 块内），mod 场景没有任何 fade。</item>
    /// <item>最后按钮 fade（if 块外，永远执行）。</item>
    /// </list>
    /// 处理方式同 GameOverOverlayPatch：postfix 把 __result 替换成包装协程，步进官方枚举器到
    /// "查表之后、按钮 fade 之前"注入 mod 结局文本；因为官方对 mod id 不做任何 fade，这里用自绘
    /// alpha 渐变（不引用 DOTween）补齐官方观感的渐显，再透传剩余 yield。
    /// </summary>
    [HarmonyPatch(typeof(EndGameController), "Start")]
    internal static class EndGameOverlayPatch
    {
        /// <summary>日志通道，由 Plugin.Awake 注入。</summary>
        internal static ManualLogSource Log;

        /// <summary>自绘渐显时长（秒），接近官方 2f 的标题/描述 DOFade 观感。</summary>
        private const float FadeDuration = 1.5f;

        private static void Postfix(EndGameController __instance, ref IEnumerator __result)
        {
            if (!ModOverlay.HasEndingContent) return;
            try
            {
                IEnumerator original = __result;
                __result = ApplyAfterOfficial(__instance, original);
            }
            catch (Exception ex)
            {
                Log?.LogWarning("End 结局 mod 文本覆盖注入失败：" + ex.Message);
            }
        }

        private static IEnumerator ApplyAfterOfficial(EndGameController controller, IEnumerator original)
        {
            try
            {
                // 步进 1：官方执行到 UnloadLoading yield（文本已清空、fade 已归零）
                if (!Step(original)) yield break;
                yield return original.Current;
                // 步进 2：官方执行到 WaitForSeconds(0.5) yield
                if (!Step(original)) yield break;
                yield return original.Current;
                // 步进 3：官方执行完 LibrarySystem.EndGame 查表（mod id 必不命中，if 块整体跳过），停在按钮 fade yield 上
                if (!Step(original)) yield break;
                // 官方对 mod id 没有任何 fade，这里写入文本并自绘渐显补齐观感
                ApplyEndingText(controller);
                yield return FadeInEndingText(controller);
                yield return original.Current;
                while (Step(original))
                    yield return original.Current;
            }
            finally
            {
                (original as IDisposable)?.Dispose();
            }
        }

        private static bool Step(IEnumerator enumerator)
        {
            try
            {
                return enumerator.MoveNext();
            }
            catch (Exception ex)
            {
                Log?.LogWarning("End 官方 Start 步进异常（终止 mod 文本覆盖，与官方协程异常行为一致）：" + ex.Message);
                return false;
            }
        }

        private static void ApplyEndingText(EndGameController controller)
        {
            try
            {
                var traverse = Traverse.Create(controller);
                Text title = traverse.Field("_titleText").GetValue<Text>();
                Text desc = traverse.Field("_descText").GetValue<Text>();
                if (title != null) title.text = ModOverlay.EndingTitle;
                if (desc != null) desc.text = ModOverlay.EndingDesc ?? "";
            }
            catch (Exception ex)
            {
                Log?.LogWarning("End 结局 mod 文本写入失败：" + ex);
            }
        }

        /// <summary>标题+描述 alpha 0→1 渐变（官方查表命中路径的 DOFade 替代品，不引用 DOTween）。</summary>
        private static IEnumerator FadeInEndingText(EndGameController controller)
        {
            Text title = null;
            Text desc = null;
            try
            {
                var traverse = Traverse.Create(controller);
                title = traverse.Field("_titleText").GetValue<Text>();
                desc = traverse.Field("_descText").GetValue<Text>();
            }
            catch (Exception ex)
            {
                Log?.LogWarning("End 结局 fade 组件读取失败：" + ex.Message);
                yield break;
            }
            if (title != null)
            {
                Color c = title.color;
                c.a = 0f;
                title.color = c;
            }
            if (desc != null)
            {
                Color c = desc.color;
                c.a = 0f;
                desc.color = c;
            }
            float elapsed = 0f;
            while (elapsed < FadeDuration)
            {
                elapsed += Time.deltaTime;
                float alpha = Mathf.Clamp01(elapsed / FadeDuration);
                if (title != null)
                {
                    Color c = title.color;
                    c.a = alpha;
                    title.color = c;
                }
                if (desc != null)
                {
                    Color c = desc.color;
                    c.a = alpha;
                    desc.color = c;
                }
                yield return null;
            }
            if (title != null)
            {
                Color c = title.color;
                c.a = 1f;
                title.color = c;
            }
            if (desc != null)
            {
                Color c = desc.color;
                c.a = 1f;
                desc.color = c;
            }
        }
    }
}
