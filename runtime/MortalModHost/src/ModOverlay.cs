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
    /// mod 死亡/结局文本覆盖（契约 §6.13/§3.1）：mod 用自造 id（9+官方id，如 910021）进官方 GameOver
    /// 场景，或在 Story 场景打开汗青书 EndGamePanel 时，LibrarySystem 查不到自造 key，画面没有文本。编译器在 mod Lua 里
    /// 发射裸全局调用 mod_set_death_text(title, desc) / mod_set_ending_text(title, desc[, image])，LuaManagerPatch 把
    /// 参数写进本类；随后进入 GameOver 或打开 EndGamePanel 时，对应 patch 用
    /// Harmony postfix 包一层官方 Start 协程，在官方"查表失败、清空文本"之后、"fade 序列开始之前"
    /// 把 mod 文本写进官方画面组件（Text/描述行 prefab），文本随官方 DOFade 渐显——布局 100% 官方。
    /// 死亡文本是官方同款"短标题 + 描述行"两段式：标题写官方标题 Text，描述按 \n 拆行注入
    /// _descContainer——绝不把整段死亡文本塞进大字号标题。
    /// 结局卡插图（契约 §3.1）：三参 mod_set_ending_text 的第三参是包内图片路径，按
    /// CurrentPackage（当前演出 mod）的 assets 表解码成 Texture2D；EndGamePanelOverlayPatch
    /// 把它写入官方汗青书左页 _picImage。
    /// 官方脚本分支演出时 Clear()（官方结局不受影响）；场景切换离开 GameOver/End 时由 Plugin.Update 清除。
    /// </summary>
    internal static class ModOverlay
    {
        /// <summary>日志通道，由 Plugin.Awake 注入（静态类拿不到插件实例的 Logger）。</summary>
        internal static ManualLogSource Log;

        /// <summary>死亡短标题（写入官方 GameOver 标题 Text；可空——只有描述时标题栏留空）。</summary>
        internal static string DeathTitle;

        /// <summary>死亡描述（可多行，中文按官方同款 \n 拆行注入 _descContainer）。</summary>
        internal static string DeathDesc;

        /// <summary>结局卡片标题（End 画面；GameOver 场景优先于死亡标题显示）。</summary>
        internal static string EndingTitle;

        /// <summary>结局卡片描述（可多行，中文按 \n 拆行）。</summary>
        internal static string EndingDesc;

        /// <summary>结局卡片背景图路径（包内 assets/ 相对路径，正斜杠；可空）。</summary>
        internal static string EndingImagePath;

        /// <summary>结局卡片背景图已解码的纹理（由 SetEnding 从包内字节解码；可空）。</summary>
        internal static Texture2D EndingTexture;

        /// <summary>本次脚本已请求显示自定义 End；即使图片损坏也要打开官方面板并回退占位图。</summary>
        internal static bool EndingRequested;

        /// <summary>由自定义纹理创建、挂到官方 EndGamePanel._picImage 的临时 Sprite。</summary>
        internal static Sprite EndingSprite;

        /// <summary>End 场景上自绘的全屏 Image（渐显完成后保留引用，Clear 时销毁）。</summary>
        internal static Image EndingImageObject;

        /// <summary>
        /// 当前演出的 mod 包（LuaManagerPatch 在 mod 脚本开演时按注册名设置）：
        /// mod_set_ending_text 的 image 参数相对它解析 assets。官方脚本演出/离开结局画面时置 null。
        /// </summary>
        internal static ModPackage CurrentPackage;

        /// <summary>GameOver 画面是否有 mod 文本可画（结局优先，其次死亡标题/描述；只有死亡描述也要画）。</summary>
        internal static bool HasGameOverContent
        {
            get
            {
                return !string.IsNullOrEmpty(EndingTitle) || !string.IsNullOrEmpty(EndingDesc)
                    || !string.IsNullOrEmpty(DeathTitle) || !string.IsNullOrEmpty(DeathDesc);
            }
        }

        /// <summary>End 画面是否有 mod 结局卡可画（标题/描述/背景图任一即可）。</summary>
        internal static bool HasEndingContent
        {
            get
            {
                return !string.IsNullOrEmpty(EndingTitle) || !string.IsNullOrEmpty(EndingDesc)
                    || EndingTexture != null || EndingRequested;
            }
        }

        /// <summary>
        /// 死亡文本两段式（契约 §6.13）：title 短标题、desc 多行描述。
        /// 单参调用兼容（老编译器/老 mod 包）：title 留空、参数当 desc——标题栏保持官方清空态，
        /// 不会把整段文本塞进大字号标题。
        /// </summary>
        internal static void SetDeathText(string title, string desc)
        {
            DeathTitle = string.IsNullOrEmpty(title) ? null : title;
            DeathDesc = string.IsNullOrEmpty(desc) ? null : desc;
        }

        internal static void SetEnding(string title, string desc, string imagePath)
        {
            EndingRequested = true;
            EndingTitle = string.IsNullOrEmpty(title) ? null : title;
            EndingDesc = string.IsNullOrEmpty(desc) ? null : desc;
            EndingImagePath = null;
            DestroyEndingTexture();
            if (string.IsNullOrEmpty(imagePath)) return;
            // 契约 §3.1：image 按"当前演出 mod"的包内 assets 解析（正斜杠统一）
            string key = imagePath.Replace('\\', '/');
            if (CurrentPackage == null)
            {
                Log?.LogWarning("mod_set_ending_text 收到 image 参数，但当前没有演出中的 mod 包（忽略图片）：" + key);
                return;
            }
            byte[] bytes;
            if (!CurrentPackage.Assets.TryGetValue(key, out bytes))
            {
                Log?.LogWarning("mod " + CurrentPackage.Id + " 的包内找不到结局卡图片 " + key + "（忽略图片，纯文字卡）");
                return;
            }
            try
            {
                var tex = new Texture2D(2, 2, TextureFormat.RGBA32, false);
                if (tex.LoadImage(bytes))
                {
                    EndingTexture = tex;
                    EndingImagePath = key;
                }
                else
                {
                    UnityEngine.Object.Destroy(tex);
                    Log?.LogWarning("mod " + CurrentPackage.Id + " 的结局卡图片解码失败（忽略图片）：" + key);
                }
            }
            catch (Exception ex)
            {
                Log?.LogWarning("mod " + CurrentPackage.Id + " 的结局卡图片解码异常（忽略图片）：" + ex.Message);
            }
        }

        internal static void Clear()
        {
            DeathTitle = null;
            DeathDesc = null;
            ClearEnding();
            CurrentPackage = null;
        }

        /// <summary>只清理本次结局卡，EndGamePanel 关闭后调用；不影响仍在运行的 mod 包上下文。</summary>
        internal static void ClearEnding()
        {
            EndingTitle = null;
            EndingDesc = null;
            EndingImagePath = null;
            EndingRequested = false;
            DestroyEndingTexture();
        }

        /// <summary>销毁自建纹理与 End 场景自绘 Image（Unity 假 null 由重载 == 防御，场景卸载后的对象安全跳过）。</summary>
        private static void DestroyEndingTexture()
        {
            if (EndingImageObject != null)
            {
                UnityEngine.Object.Destroy(EndingImageObject);
                EndingImageObject = null;
            }
            if (EndingSprite != null)
            {
                UnityEngine.Object.Destroy(EndingSprite);
                EndingSprite = null;
            }
            if (EndingTexture != null)
            {
                UnityEngine.Object.Destroy(EndingTexture);
                EndingTexture = null;
            }
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
            if (ModDisclosure.Active)
            {
                Transform buttonAnchor = ResolveButtonDisclosureAnchor(__instance);
                GameObject disclosure = buttonAnchor != null
                    ? ModDisclosure.AttachToPanel(
                        buttonAnchor,
                        Vector2.one,
                        new Vector2(1f, 0f),
                        new Vector2(0f, 14f))
                    : ModDisclosure.AttachToPanel(ResolveDisclosureAnchor(__instance));
                if (disclosure == null)
                {
                    (__result as IDisposable)?.Dispose();
                    __result = ModDisclosure.EmptyRoutine();
                    LuaManagerPatch.AbortActivePlayback(
                        "GameOver 卡片无法附加强制玩家内容标记",
                        null, null, "mandatory_disclosure");
                    return;
                }
            }
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

        /// <summary>
        /// 原版 GameOver 的标题父节点 Vertical/Horizontal 实际铺满全屏；挂到它的
        /// “右上角”会与常驻边缘标完全重叠。标题按钮画布是卡片自身右下角的 300x100
        /// 区域，局部标识放在按钮上沿，截图裁取死亡卡或返回按钮时都会保留。
        /// </summary>
        private static Transform ResolveButtonDisclosureAnchor(GameOverController controller)
        {
            try
            {
                CanvasGroup buttons = Traverse.Create(controller)
                    .Field("_titleButtonCanvas").GetValue<CanvasGroup>();
                if (buttons != null) return buttons.transform;
            }
            catch { }
            return null;
        }

        private static Transform ResolveDisclosureAnchor(GameOverController controller)
        {
            try
            {
                var traverse = Traverse.Create(controller);
                Text title = SystemSettings.IsChineseLanguage
                    ? traverse.Field("_titleText").GetValue<Text>()
                    : traverse.Field("_horizontalTitleText").GetValue<Text>();
                if (title != null)
                    return title.rectTransform.parent != null ? title.rectTransform.parent : title.rectTransform;
            }
            catch { }
            return controller.transform;
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
        /// 优先级（契约 §6.13 两段式）：标题 = 结局 title &gt; 死亡 title（都空则标题栏留空）；
        /// 描述 = 结局 desc &gt; 死亡 desc；死亡只有 desc 时只显示描述行，不塞标题。
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

                // GameOver 优先级（契约 §6.13 两段式）：标题 = 结局 title > 死亡 title；描述 = 结局 desc > 死亡 desc。
                // 死亡只有 desc 时标题留空只显示描述行（避免整段塞标题）。
                string titleText = !string.IsNullOrEmpty(ModOverlay.EndingTitle)
                    ? ModOverlay.EndingTitle
                    : ModOverlay.DeathTitle;
                if (title != null) title.text = titleText ?? "";
                if (horizontalTitle != null) horizontalTitle.text = titleText ?? "";

                string desc = !string.IsNullOrEmpty(ModOverlay.EndingDesc)
                    ? ModOverlay.EndingDesc
                    : ModOverlay.DeathDesc;
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
    /// 原版“汗青书”结局卡 patch。官方剧情不是切换到 End 场景来画这张卡，而是在 Story
    /// 场景中执行 <c>runwait(endgamepanel.Open("200xx"))</c>：EndGamePanel 自带书卷版式、
    /// 插图槽、标题/正文渐显和等待确认。编译器给 mod 传固定的不存在 key，官方查表不会
    /// 解锁/写入任何官方结局；本包装协程在第一次 yield（画布渐显）前注入自定义内容。
    /// 未指定 image 时临时借用官方 20047 的 Picture 作占位，不复制或分发游戏资源。
    /// </summary>
    [HarmonyPatch(typeof(EndGamePanel), "Open")]
    internal static class EndGamePanelOverlayPatch
    {
        internal static ManualLogSource Log;
        private const string PlaceholderEndingKey = "20047";

        private static void Postfix(EndGamePanel __instance, ref IEnumerator __result)
        {
            if (ModDisclosure.Active)
            {
                if (ModDisclosure.AttachToPanel(ResolveDisclosureAnchor(__instance)) == null)
                {
                    (__result as IDisposable)?.Dispose();
                    __result = ModDisclosure.EmptyRoutine();
                    LuaManagerPatch.AbortActivePlayback(
                        "汗青书结局卡无法附加强制玩家内容标记",
                        null, null, "mandatory_disclosure");
                    return;
                }
            }
            if (!ModOverlay.HasEndingContent) return;
            try
            {
                IEnumerator original = __result;
                __result = ApplyAfterOfficial(__instance, original);
            }
            catch (Exception ex)
            {
                Log?.LogWarning("汗青书 mod 结局卡注入失败：" + ex.Message);
            }
        }

        private static Transform ResolveDisclosureAnchor(EndGamePanel panel)
        {
            try
            {
                Text title = Traverse.Create(panel).Field("_titleText").GetValue<Text>();
                if (title != null)
                    return title.rectTransform.parent != null ? title.rectTransform.parent : title.rectTransform;
            }
            catch { }
            return panel.transform;
        }

        private static IEnumerator ApplyAfterOfficial(EndGamePanel panel, IEnumerator original)
        {
            bool restoreSaveLibrary = false;
            bool saveLibrary = false;
            try
            {
                // Open 首次 MoveNext 前会清空文字、查 LibrarySystem、设置交互，
                // 然后停在 _canvasGroup.DOFade。此处写入即可完整沿用后续官方渐显。
                if (!Step(original)) yield break;
                try
                {
                    var traverse = Traverse.Create(panel);
                    saveLibrary = traverse.Field("_saveLibrary").GetValue<bool>();
                    if (saveLibrary)
                    {
                        // mod key 不在 LibrarySystem，禁止打开无法解析条目的存档槽面板。
                        traverse.Field("_saveLibrary").SetValue(false);
                        restoreSaveLibrary = true;
                    }
                    ApplyEnding(panel);
                }
                catch (Exception ex)
                {
                    Log?.LogWarning("汗青书 mod 标题/图片写入失败：" + ex);
                }
                yield return original.Current;
                while (Step(original))
                    yield return original.Current;
            }
            finally
            {
                if (restoreSaveLibrary)
                {
                    try
                    {
                        Traverse.Create(panel).Field("_saveLibrary").SetValue(saveLibrary);
                    }
                    catch (Exception ex)
                    {
                        Log?.LogWarning("恢复 EndGamePanel._saveLibrary 失败：" + ex.Message);
                    }
                }
                (original as IDisposable)?.Dispose();
                ModOverlay.ClearEnding();
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
                Log?.LogWarning("汗青书官方 Open 步进异常（终止本次结局卡）：" + ex.Message);
                return false;
            }
        }

        private static void ApplyEnding(EndGamePanel panel)
        {
            var traverse = Traverse.Create(panel);
            Text title = traverse.Field("_titleText").GetValue<Text>();
            Text desc = traverse.Field("_descText").GetValue<Text>();
            Image picture = traverse.Field("_picImage").GetValue<Image>();
            if (title != null) title.text = ModOverlay.EndingTitle ?? "";
            if (desc != null) desc.text = ModOverlay.EndingDesc ?? "";
            if (picture == null) return;

            Sprite sprite = null;
            if (ModOverlay.EndingTexture != null)
            {
                sprite = Sprite.Create(
                    ModOverlay.EndingTexture,
                    new Rect(0f, 0f, ModOverlay.EndingTexture.width, ModOverlay.EndingTexture.height),
                    new Vector2(0.5f, 0.5f));
                ModOverlay.EndingSprite = sprite;
            }
            else
            {
                // 本地直接借用已加载的原版 LibraryItemData 图片，只作开发占位。
                LibraryItemData placeholder = LibrarySystem.Instance.EndGame.Get(PlaceholderEndingKey);
                if (placeholder != null) sprite = placeholder.Picture;
                if (sprite == null)
                    Log?.LogWarning("找不到原版结局 " + PlaceholderEndingKey + " 的插图，占位图将留空");
            }
            picture.sprite = sprite;
            picture.preserveAspect = true;
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
            if (ModDisclosure.Active)
            {
                Transform buttonAnchor = ResolveButtonDisclosureAnchor(__instance);
                GameObject disclosure = buttonAnchor != null
                    ? ModDisclosure.AttachToPanel(
                        buttonAnchor,
                        Vector2.one,
                        new Vector2(1f, 0f),
                        new Vector2(0f, 14f))
                    : ModDisclosure.AttachToPanel(ResolveDisclosureAnchor(__instance));
                if (disclosure == null)
                {
                    (__result as IDisposable)?.Dispose();
                    __result = ModDisclosure.EmptyRoutine();
                    LuaManagerPatch.AbortActivePlayback(
                        "End 结局卡无法附加强制玩家内容标记",
                        null, null, "mandatory_disclosure");
                    return;
                }
            }
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

        private static Transform ResolveButtonDisclosureAnchor(EndGameController controller)
        {
            try
            {
                CanvasGroup buttons = Traverse.Create(controller)
                    .Field("_titleButtonCanvas").GetValue<CanvasGroup>();
                if (buttons != null) return buttons.transform;
            }
            catch { }
            return null;
        }

        private static Transform ResolveDisclosureAnchor(EndGameController controller)
        {
            try
            {
                Text title = Traverse.Create(controller).Field("_titleText").GetValue<Text>();
                if (title != null)
                    return title.rectTransform.parent != null ? title.rectTransform.parent : title.rectTransform;
            }
            catch { }
            return controller.transform;
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
                // 结局卡背景图（契约 §3.1）：官方 End 场景 Canvas 上垫底全屏 Image，
                // alpha 初始 0，由 FadeInEndingText 与文字同速渐显。
                if (ModOverlay.EndingTexture != null)
                {
                    try
                    {
                        ApplyEndingImage(controller, title);
                    }
                    catch (Exception ex)
                    {
                        Log?.LogWarning("End 结局卡背景图创建失败（纯文字卡）：" + ex.Message);
                        if (ModOverlay.EndingImageObject != null)
                        {
                            UnityEngine.Object.Destroy(ModOverlay.EndingImageObject);
                            ModOverlay.EndingImageObject = null;
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                Log?.LogWarning("End 结局 mod 文本写入失败：" + ex);
            }
        }

        /// <summary>
        /// 在官方 End 场景 Canvas 上创建全屏 Image 垫底：挂在 Canvas 根下并 SetAsFirstSibling
        /// （渲染在全部 UI 之后，标题/描述文字在上）；锚点拉伸填满画布、preserveAspect 保持比例；
        /// raycastTarget=false 不拦截「回主選單」按钮点击。返回的 Image 引用存 ModOverlay.EndingImageObject。
        /// </summary>
        private static void ApplyEndingImage(EndGameController controller, Text title)
        {
            Transform root = null;
            if (title != null && title.canvas != null) root = title.canvas.transform;
            if (root == null && title != null) root = title.transform.parent;
            if (root == null)
            {
                Log?.LogWarning("End 结局卡背景图：找不到可用的 Canvas（_titleText 不在 Canvas 下）");
                return;
            }
            var go = new GameObject("mod_ending_image", typeof(RectTransform));
            var rect = (RectTransform)go.transform;
            rect.SetParent(root, false);
            rect.anchorMin = Vector2.zero;
            rect.anchorMax = Vector2.one;
            rect.offsetMin = Vector2.zero;
            rect.offsetMax = Vector2.zero;
            go.transform.SetAsFirstSibling();
            var image = go.AddComponent<Image>();
            image.sprite = Sprite.Create(
                ModOverlay.EndingTexture,
                new Rect(0f, 0f, ModOverlay.EndingTexture.width, ModOverlay.EndingTexture.height),
                new Vector2(0.5f, 0.5f));
            ModOverlay.EndingSprite = image.sprite;
            image.color = new Color(1f, 1f, 1f, 0f);
            image.raycastTarget = false;
            image.preserveAspect = true;
            ModOverlay.EndingImageObject = image;
        }

        /// <summary>标题+描述 alpha 0→1 渐变（官方查表命中路径的 DOFade 替代品，不引用 DOTween）；有背景图时图同速渐显。</summary>
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
            Image endingImage = ModOverlay.EndingImageObject;
            if (endingImage != null)
            {
                Color c = endingImage.color;
                c.a = 0f;
                endingImage.color = c;
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
                if (endingImage != null)
                {
                    Color c = endingImage.color;
                    c.a = alpha;
                    endingImage.color = c;
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
            if (endingImage != null)
            {
                Color c = endingImage.color;
                c.a = 1f;
                endingImage.color = c;
            }
        }
    }
}
