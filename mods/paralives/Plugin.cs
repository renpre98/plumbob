using System;
using System.Collections;
using System.Collections.Generic;
using System.Net;
using System.Reflection;
using System.Text;
using BepInEx;
using BepInEx.Configuration;
using BepInEx.Logging;
using HarmonyLib;
using UnityEngine;

namespace PlumbobLink
{
    [BepInPlugin(PluginGuid, PluginName, PluginVersion)]
    public class Plugin : BaseUnityPlugin
    {
        public const string PluginGuid = "io.github.renpre98.plumboblink";
        public const string PluginName = "Plumbob Link";
        public const string PluginVersion = "1.0.1";

        internal static ManualLogSource Log;
        internal static ConfigEntry<string> CfgDaemonUrl;
        internal static ConfigEntry<int> CfgFadeMs;
        internal static ConfigEntry<float> CfgMinPostInterval;

        // last (characterGUID, dominantEmotionGUID) we POSTed; suppress duplicates.
        internal static ulong LastCharacter;
        internal static ulong LastEmotion;
        internal static float LastPostTime;

        // Only nag once when the daemon is unreachable; flip back to single-warn after a success.
        private static bool _hasWarnedDaemonDown;

        private void Awake()
        {
            Log = Logger;
            CfgDaemonUrl = Config.Bind("Daemon", "Url",
                "http://127.0.0.1:27301/emotion",
                "Endpoint of plumbob-daemon");
            CfgFadeMs = Config.Bind("Daemon", "FadeMs", 800,
                "Fade duration sent to the daemon");
            CfgMinPostInterval = Config.Bind("Throttling", "MinIntervalSeconds", 0.5f,
                "Minimum seconds between two POSTs for the same character");

            var harmony = new Harmony(PluginGuid);
            harmony.PatchAll(Assembly.GetExecutingAssembly());
            Log.LogInfo($"{PluginName} {PluginVersion} loaded; posting to {CfgDaemonUrl.Value}");
        }

        internal static void TryPost(string emotionName, Color color)
        {
            try
            {
                var req = (HttpWebRequest)WebRequest.Create(CfgDaemonUrl.Value);
                req.Method = "POST";
                req.ContentType = "application/json";
                req.Timeout = 250; // milliseconds — must never block the render thread for long

                // The daemon understands {emotion: "Happy"} and falls back to its own palette
                // if the name is unknown; we also pass the in-game RGB so it can use that
                // verbatim if it prefers — daemon-side will be enhanced to honor "rgb" if
                // present, but the basic /emotion route ignores it today and uses the name.
                int r = (int)Math.Round(Mathf.Clamp01(color.r) * 255f);
                int g = (int)Math.Round(Mathf.Clamp01(color.g) * 255f);
                int b = (int)Math.Round(Mathf.Clamp01(color.b) * 255f);
                var json = "{\"game\":\"paralives\",\"emotion\":\"" + Escape(emotionName) +
                           "\",\"fade_ms\":" + CfgFadeMs.Value +
                           ",\"rgb\":[" + r + "," + g + "," + b + "]}";

                using (var s = req.GetRequestStream())
                {
                    var bytes = Encoding.UTF8.GetBytes(json);
                    s.Write(bytes, 0, bytes.Length);
                }
                // We don't read the response — fire-and-forget; close immediately.
                req.GetResponse().Close();
                if (_hasWarnedDaemonDown)
                {
                    Log.LogMessage("plumbob-daemon is reachable again.");
                    _hasWarnedDaemonDown = false;
                }
            }
            catch (WebException we) when (we.Status == WebExceptionStatus.ConnectFailure
                                       || we.Status == WebExceptionStatus.Timeout
                                       || we.Status == WebExceptionStatus.ConnectionClosed)
            {
                if (!_hasWarnedDaemonDown)
                {
                    Log.LogWarning(
                        $"plumbob-daemon not reachable at {CfgDaemonUrl.Value} ({we.Status}). " +
                        "Start it with: ~/.local/bin/plumbob-daemon  " +
                        "(or: systemctl --user start plumbob-daemon). Suppressing further warnings.");
                    _hasWarnedDaemonDown = true;
                }
            }
            catch (Exception e)
            {
                Log.LogDebug($"POST failed: {e.Message}");
            }
        }

        private static string Escape(string s)
        {
            if (string.IsNullOrEmpty(s)) return "";
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"");
        }
    }

    /// <summary>
    /// Postfix on UIEmotions2.Update — runs each frame the UI updates the emotion bar.
    /// We read the private `_previousCharacter` and `_emotionValues` fields and post the
    /// dominant emotion to the local daemon. Throttled and de-duplicated.
    /// </summary>
    [HarmonyPatch]
    internal static class UIEmotionsUpdatePatch
    {
        private static Type _uiType;
        private static FieldInfo _fPrevChar;
        private static FieldInfo _fEmotionValues;
        private static bool _dumpDone;
        private static Dictionary<ulong, (string name, Color color)> _emotionLookup;

        static bool Prepare()
        {
            _uiType = AccessTools.TypeByName("UIEmotions2");
            if (_uiType == null)
            {
                Plugin.Log?.LogWarning("UIEmotions2 type not found — Paralives may have renamed it.");
                return false;
            }
            _fPrevChar = AccessTools.Field(_uiType, "_previousCharacter");
            _fEmotionValues = AccessTools.Field(_uiType, "_emotionValues");
            return _fPrevChar != null && _fEmotionValues != null;
        }

        static MethodBase TargetMethod() => AccessTools.Method(_uiType, "Update");

        static void Postfix(object __instance)
        {
            try
            {
                if (!_dumpDone) TryDumpEmotionRegistry();

                ulong character = (ulong)_fPrevChar.GetValue(__instance);
                if (character == 0UL) return;

                var emotions = _fEmotionValues.GetValue(__instance) as IList;
                if (emotions == null || emotions.Count == 0) return;

                // The list is sorted ascending — the dominant emotion is at the END.
                // Items are ValueTuple<ulong, int, int>; read via reflection because
                // tuples are awkward to cast across assemblies.
                var item = emotions[emotions.Count - 1];
                var itemType = item.GetType();
                var emotionGuid = (ulong)itemType.GetField("Item1").GetValue(item);
                var value = (int)itemType.GetField("Item2").GetValue(item);

                // Suppress duplicates and throttle.
                if (character == Plugin.LastCharacter && emotionGuid == Plugin.LastEmotion) return;
                if (Time.unscaledTime - Plugin.LastPostTime < Plugin.CfgMinPostInterval.Value) return;

                string name = ResolveEmotionName(emotionGuid, out Color color);
                if (string.IsNullOrEmpty(name)) return;

                Plugin.LastCharacter = character;
                Plugin.LastEmotion = emotionGuid;
                Plugin.LastPostTime = Time.unscaledTime;
                Plugin.Log?.LogInfo($"para={character} emotion={name} guid={emotionGuid} value={value}");
                Plugin.TryPost(name, color);
            }
            catch (Exception e)
            {
                Plugin.Log?.LogDebug($"postfix error: {e.Message}");
            }
        }

        /// <summary>
        /// Resolve emotion GUID → (DisplayName, BackgroundColor). We don't have a documented
        /// API for this yet, so we scan loaded assemblies once for Setting.Emotion instances
        /// reachable through any static field. If that fails we just send the GUID as the
        /// emotion name and let the daemon fall back to its palette.
        /// </summary>
        private static string ResolveEmotionName(ulong guid, out Color color)
        {
            color = Color.white;
            if (_emotionLookup != null && _emotionLookup.TryGetValue(guid, out var hit))
            {
                color = hit.color;
                return hit.name;
            }
            return guid.ToString();
        }

        /// One-shot: enumerate every Setting.Emotion via Settings.Instance.SettingObjects
        /// and log (GUID, DisplayName, BackgroundColor). Setting.Emotions is a POCO, not a
        /// UnityEngine.Object, so we can't use Resources.FindObjectsOfTypeAll for it — the
        /// authoritative container is the global Settings singleton.
        private static void TryDumpEmotionRegistry()
        {
            // Always mark as done — single attempt only, no per-frame retries.
            _dumpDone = true;
            try
            {
                var settingsType = AccessTools.TypeByName("Settings");
                var emotionsType = AccessTools.TypeByName("Setting.Emotions");
                var emotionType = AccessTools.TypeByName("Setting.Emotion");
                if (settingsType == null || emotionsType == null || emotionType == null)
                {
                    Plugin.Log?.LogWarning("dump: required types missing");
                    return;
                }

                var instGetter = AccessTools.PropertyGetter(settingsType, "Instance");
                var instance = instGetter?.Invoke(null, null);
                if (instance == null)
                {
                    Plugin.Log?.LogWarning("dump: Settings.Instance is null (too early?)");
                    _dumpDone = false; // legit "not yet" — allow one more try later
                    return;
                }

                var soGetter = AccessTools.PropertyGetter(settingsType, "SettingObjects");
                var soDict = soGetter?.Invoke(instance, null) as IDictionary;
                if (soDict == null)
                {
                    Plugin.Log?.LogWarning("dump: SettingObjects not a dictionary");
                    return;
                }

                if (!soDict.Contains(emotionsType))
                {
                    Plugin.Log?.LogWarning("dump: SettingObjects has no entry for Setting.Emotions");
                    return;
                }
                var emotionsInst = soDict[emotionsType];

                var fAll = AccessTools.Field(emotionsType, "AllEmotions");
                var fGuid = AccessTools.Field(emotionType, "GUID");
                var fName = AccessTools.Field(emotionType, "DisplayName");
                var fColor = AccessTools.Field(emotionType, "BackgroundColor");
                if (fAll == null || fGuid == null || fName == null || fColor == null)
                {
                    Plugin.Log?.LogWarning("dump: field reflection failed (renamed?)");
                    return;
                }

                var arr = fAll.GetValue(emotionsInst) as Array;
                if (arr == null) { Plugin.Log?.LogWarning("dump: AllEmotions is null"); return; }

                var lookup = new Dictionary<ulong, (string, Color)>();
                foreach (var emo in arr)
                {
                    if (emo == null) continue;
                    ulong guid = (ulong)fGuid.GetValue(emo);
                    string name = (string)fName.GetValue(emo) ?? "";
                    Color color = (Color)fColor.GetValue(emo);
                    lookup[guid] = (name, color);
                }

                _emotionLookup = lookup;
                Plugin.Log?.LogMessage($"emotion registry dumped ({lookup.Count} entries):");
                foreach (var kv in lookup)
                {
                    var (name, color) = kv.Value;
                    int r = (int)Math.Round(Mathf.Clamp01(color.r) * 255f);
                    int g = (int)Math.Round(Mathf.Clamp01(color.g) * 255f);
                    int b = (int)Math.Round(Mathf.Clamp01(color.b) * 255f);
                    Plugin.Log?.LogMessage(
                        $"  emotion guid={kv.Key} name=\"{name}\" rgb=#{r:X2}{g:X2}{b:X2}");
                }
            }
            catch (Exception e)
            {
                Plugin.Log?.LogWarning($"emotion-registry dump failed: {e}");
            }
        }
    }
}
