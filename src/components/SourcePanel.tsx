// 在线源：详情栏里的“在线源”区块，以及关联 / 从在线源添加用的对话框。

import { ArrowUpCircle, Check, Download, ExternalLink, LoaderCircle, RefreshCw, Search, Unlink, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { errorText } from "../lib/i18n";
import { assetInLibrary, formatSize, hasUpdate } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { Asset, Candidate, Release, Software, SourceKind } from "../lib/types";
import { confirm } from "./dialogs";
import { PlatformChip, PlatformIcons } from "./PlatformIcon";
import { Button, Chip, Field, IconButton, Modal, Segmented, TextInput, cx } from "./ui";

const KINDS: SourceKind[] = ["homebrew", "github", "winget"];

/** 详情栏里的区块：没关联时给一个入口，关联后显示最新版本和可以下载的安装包。 */
export function SourceSection({ software }: { software: Software }) {
  const t = useT();
  const apply = useApp((state) => state.apply);
  const attempt = useApp((state) => state.attempt);
  const patch = useApp((state) => state.patch);
  const [checking, setChecking] = useState(false);
  const subscription = software.subscription;

  if (!subscription) {
    return (
      <div>
        <Button onClick={() => patch({ sourceDialog: { softwareId: software.id } })}>{t("source.link")}</Button>
        <p className="m-0 mt-1.5 text-[12px] text-fg-4">{t("source.linkHint")}</p>
      </div>
    );
  }

  const latest = subscription.latest;
  const check = async () => {
    setChecking(true);
    await apply(() => api.subscribe(software.id, null, subscription.source, subscription.reference).then((r) => r.library));
    setChecking(false);
  };
  const unlink = async () => {
    const { ok } = await confirm({
      title: t("source.unlink"),
      body: t("source.unlinkConfirm", { name: software.name }),
      confirmLabel: t("source.unlink"),
    });
    if (ok) await apply(() => api.unsubscribe(software.id));
  };

  return (
    <div className="rounded-lg border border-line">
      <div className="flex h-8 items-center gap-1.5 border-b border-line pr-1 pl-2.5">
        <Chip>{t(`source.${subscription.source}`)}</Chip>
        <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px] text-fg-2" title={subscription.reference}>
          {subscription.reference}
        </span>
        <IconButton label={t("source.check")} className="size-6" disabled={checking} onClick={() => void check()}>
          <RefreshCw size={13} strokeWidth={1.75} className={checking ? "animate-spin" : ""} />
        </IconButton>
        <IconButton label={t("source.unlink")} className="size-6" onClick={() => void unlink()}>
          <Unlink size={13} strokeWidth={1.75} />
        </IconButton>
      </div>
      {latest && (
        <>
          <div className="flex items-center gap-2 px-2.5 py-2">
            <span className="tabular text-[13px] font-medium">{t("source.latest", { version: latest.version })}</span>
            {hasUpdate(software) ? (
              <Chip className="bg-accent-soft text-accent">{t("source.update")}</Chip>
            ) : (
              <span className="text-[12px] text-fg-3">{t("source.upToDate")}</span>
            )}
            {latest.page && (
              <IconButton label={t("source.page")} className="ml-auto size-6" onClick={() => void attempt(() => api.openUrl(latest.page))}>
                <ExternalLink size={13} strokeWidth={1.75} />
              </IconButton>
            )}
          </div>
          {latest.assets.length === 0 && <div className="border-t border-line px-2.5 py-2 text-[12px] text-fg-4">{t("source.noAssets")}</div>}
          {latest.assets.map((asset) => (
            <AssetRow key={asset.url} software={software} release={latest} asset={asset} />
          ))}
        </>
      )}
    </div>
  );
}

function AssetRow({ software, release, asset }: { software: Software; release: Release; asset: Asset }) {
  const t = useT();
  const download = useApp((state) => state.downloads[asset.url]);
  const start = useApp((state) => state.downloadAsset);
  const owned = assetInLibrary(software, release, asset);
  const percent = download?.total ? Math.min(100, Math.round((download.received / download.total) * 100)) : null;

  return (
    <div className="relative flex items-center gap-1 border-t border-line py-1.5 pr-1 pl-2.5">
      <div className="min-w-0 flex-1">
        <div className="selectable truncate text-[12px]" title={asset.url}>
          {asset.name}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-1 text-[12px] text-fg-3">
          <PlatformChip platform={asset.platform} />
          {asset.arch && <Chip>{asset.arch}</Chip>}
          {asset.size !== null && <span className="tabular ml-0.5">{formatSize(asset.size)}</span>}
        </div>
      </div>
      {download ? (
        <>
          <span className="tabular shrink-0 text-[12px] text-accent">
            {download.received === 0 ? t("source.queued") : percent !== null ? `${percent}%` : formatSize(download.received)}
          </span>
          <IconButton label={t("source.cancel")} onClick={() => void api.cancelDownload(asset.url)}>
            <X size={15} strokeWidth={1.75} />
          </IconButton>
          <div className="absolute inset-x-0 bottom-0 h-0.5 bg-line">
            <div className="h-full bg-accent transition-[width] duration-200" style={{ width: `${percent ?? 0}%` }} />
          </div>
        </>
      ) : owned ? (
        <span title={t("source.inLibrary")} aria-label={t("source.inLibrary")} className="flex size-7 shrink-0 items-center justify-center text-success">
          <Check size={15} strokeWidth={2} />
        </span>
      ) : (
        <IconButton label={t("source.download")} onClick={() => void start(software.id, asset)}>
          <Download size={15} strokeWidth={1.75} />
        </IconButton>
      )}
    </div>
  );
}

/** 列表里“有新版本”的小标记。 */
export function UpdateMark({ software }: { software: Software }) {
  const t = useT();
  const latest = software.subscription?.latest;
  if (!latest || !hasUpdate(software)) return null;
  const label = `${t("source.update")} · ${latest.version}`;
  return (
    <span title={label} aria-label={label} className="flex shrink-0 text-accent">
      <ArrowUpCircle size={13} strokeWidth={2} />
    </span>
  );
}

/** 关联在线源，或者从在线源新添一个软件。由 `sourceDialog` 状态控制开关。 */
export function SourceDialog() {
  const target = useApp((state) => state.sourceDialog);
  if (!target) return null;
  return <Dialog key={target.softwareId ?? "new"} softwareId={target.softwareId} />;
}

function Dialog({ softwareId }: { softwareId: string | null }) {
  const t = useT();
  const language = useLanguage();
  const software = useApp((state) => state.library?.software.find((s) => s.id === softwareId));
  const patch = useApp((state) => state.patch);
  const apply = useApp((state) => state.apply);
  const select = useApp((state) => state.select);
  const setRoute = useApp((state) => state.setRoute);
  const close = () => patch({ sourceDialog: null });

  const [kind, setKind] = useState<SourceKind>("homebrew");
  const [query, setQuery] = useState(software?.name ?? "");
  const [results, setResults] = useState<Candidate[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [reference, setReference] = useState("");
  const [name, setName] = useState("");
  const [preview, setPreview] = useState<Release | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  // Homebrew：输入停下来一会儿就搜
  useEffect(() => {
    if (kind !== "homebrew" || query.trim().length < 2) {
      setResults(null);
      return;
    }
    let stale = false;
    setSearching(true);
    const timer = setTimeout(() => {
      api.sourceSearch(query.trim()).then(
        (found) => {
          if (stale) return;
          setResults(found);
          setSearching(false);
          setProblem(null);
        },
        (error) => {
          if (stale) return;
          setSearching(false);
          setProblem(errorText(language, error));
        },
      );
    }, 350);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [kind, query, language]);

  const switchKind = (next: SourceKind) => {
    setKind(next);
    setReference("");
    setPreview(null);
    setProblem(null);
  };

  /** 查这个标识对应的最新版本，查到了才能关联。 */
  const lookup = async (ref: string, suggestedName?: string) => {
    setReference(ref);
    setPreview(null);
    setProblem(null);
    if (!ref.trim()) return;
    setBusy(true);
    try {
      setPreview(await api.sourceLookup(kind, ref.trim()));
      // 新添软件时给个默认名字：搜索结果里的名字，或者标识的最后一段
      if (!softwareId) setName(suggestedName ?? ref.trim().replace(/\/+$/, "").split(/[/.]/).pop() ?? ref);
    } catch (error) {
      setProblem(errorText(language, error));
    } finally {
      setBusy(false);
    }
  };

  const confirmLink = async () => {
    if (!preview || (!softwareId && !name.trim())) return;
    setBusy(true);
    let addedId: string | null = null;
    const ok = await apply(async () => {
      const result = await api.subscribe(softwareId, softwareId ? null : name.trim(), kind, reference.trim());
      addedId = result.softwareId;
      return result.library;
    });
    setBusy(false);
    if (!ok) return;
    close();
    if (addedId) {
      select(addedId);
      setRoute("library");
    }
  };

  return (
    <Modal
      open
      onClose={close}
      title={softwareId ? t("source.dialog.link") : t("source.dialog.add")}
      width={500}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy && preview !== null} disabled={!preview || (!softwareId && !name.trim())} onClick={() => void confirmLink()}>
            {softwareId ? t("source.confirmLink") : t("source.confirmAdd")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <Segmented<SourceKind> value={kind} onChange={switchKind} options={KINDS.map((value) => ({ value, label: t(`source.${value}`) }))} />
        </div>
        <p className="m-0 text-[12px] leading-relaxed text-fg-3">
          {t(`source.hint.${kind}`)}
          {kind !== "homebrew" && ` ${t("source.rateHint")}`}
        </p>

        {kind === "homebrew" ? (
          <>
            <div className="relative">
              <Search size={14} strokeWidth={1.75} aria-hidden className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-fg-3" />
              <TextInput autoFocus value={query} placeholder={t("source.search")} onChange={(event) => setQuery(event.target.value)} className="pl-8" />
            </div>
            <div className="max-h-[188px] min-h-[64px] overflow-y-auto rounded-lg border border-line">
              {searching && (
                <div className="flex items-center gap-2 px-2.5 py-2 text-[12px] text-fg-3">
                  <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
                  {results === null ? t("source.firstSearch") : t("source.searching")}
                </div>
              )}
              {!searching && results?.length === 0 && <div className="px-2.5 py-2 text-[12px] text-fg-4">{t("source.noResults")}</div>}
              {results?.map((candidate) => (
                <button
                  key={candidate.reference}
                  type="button"
                  aria-pressed={candidate.reference === reference}
                  onClick={() => void lookup(candidate.reference, candidate.name)}
                  className={cx(
                    "flex w-full items-baseline gap-2 border-b border-line px-2.5 py-1.5 text-left last:border-b-0",
                    candidate.reference === reference ? "bg-accent-soft" : "hover:bg-hover",
                  )}
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px]">{candidate.name}</span>
                    <span className="block truncate text-[12px] text-fg-3">{candidate.description || candidate.reference}</span>
                  </span>
                  <span className="tabular shrink-0 text-[12px] text-fg-3">{candidate.version}</span>
                </button>
              ))}
            </div>
          </>
        ) : (
          <div className="flex gap-2">
            <TextInput
              autoFocus
              value={reference}
              placeholder={t(`source.ref.${kind}`)}
              onChange={(event) => {
                setReference(event.target.value);
                setPreview(null);
              }}
              onKeyDown={(event) => event.key === "Enter" && void lookup(reference)}
            />
            <Button busy={busy && !preview} disabled={!reference.trim()} onClick={() => void lookup(reference)}>
              {t("source.preview")}
            </Button>
          </div>
        )}

        {problem && <div className="text-[12px] text-danger">{problem}</div>}
        {busy && !preview && kind === "homebrew" && (
          <div className="flex items-center gap-2 text-[12px] text-fg-3">
            <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
            {reference}
          </div>
        )}

        {preview && (
          <div className="rounded-lg border border-line">
            <div className="flex items-center gap-2 px-2.5 py-2 text-[13px]">
              <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px] text-fg-2">{reference}</span>
              <span className="tabular shrink-0 font-medium">{t("source.latest", { version: preview.version })}</span>
            </div>
            {preview.assets.length === 0 && <div className="border-t border-line px-2.5 py-2 text-[12px] text-fg-4">{t("source.noAssets")}</div>}
            <div className="max-h-[132px] overflow-y-auto">
              {preview.assets.map((asset) => (
                <div key={asset.url} className="flex items-center gap-2 border-t border-line px-2.5 py-1.5 text-[12px]">
                  <span className="min-w-0 flex-1 truncate">{asset.name}</span>
                  <PlatformIcons platforms={[asset.platform]} size={13} />
                  {asset.arch && <Chip>{asset.arch}</Chip>}
                </div>
              ))}
            </div>
          </div>
        )}

        {preview && !softwareId && (
          <Field label={t("source.name")}>
            <TextInput value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
        )}
      </div>
    </Modal>
  );
}
