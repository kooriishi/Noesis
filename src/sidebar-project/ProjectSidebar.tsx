import React, { useEffect, useMemo, useState } from 'react';
import {
  Folder,
  FolderOpen,
  FileCode,
  FileText,
  Map,
  Compass,
  FolderPlus,
  RefreshCw,
  CheckCircle,
  Circle,
  ChevronRight,
  ChevronDown,
  ArrowRight,
  Search,
  Cpu,
  Bot,
  GitBranch,
  Layers,
  Clock,
  SkipForward,
} from 'lucide-react';
import { FileNode, GraphEdge, GraphEntity, ProjectMap, ReadingStepStatus } from '../types';
import { ApiService } from '../services/api';

interface ProjectSidebarProps {
  currentProject: FileNode | null;
  projectMap: ProjectMap | null;
  activeFilePath: string;
  onSelectFile: (filePath: string, line?: number) => void;
  onOpenProject: () => void;
  onTriggerIndex: () => void;
  isIndexing: boolean;
}

type MapSubView = 'modules' | 'calls' | 'ros2' | 'llm_app';

const STAGE_LABELS: Record<string, { title: string; color: string; bg: string }> = {
  stage_1_entry: {
    title: '阶段 1 · 入口与启动层 (Entry & Launch)',
    color: '#f9e2af',
    bg: 'rgba(249, 226, 175, 0.14)',
  },
  stage_2_backbone: {
    title: '阶段 2 · 核心协调与主干循环 (Core Backbone)',
    color: '#89b4fa',
    bg: 'rgba(137, 180, 250, 0.14)',
  },
  stage_3_domain: {
    title: '阶段 3 · 关键业务与通信节点 (Domain & Comm)',
    color: '#cba6f7',
    bg: 'rgba(203, 166, 247, 0.14)',
  },
  stage_4_leaf: {
    title: '阶段 4 · 底层工具与数据结构 (Leaf & Types)',
    color: '#a6e3a1',
    bg: 'rgba(166, 227, 161, 0.14)',
  },
};

export const ProjectSidebar: React.FC<ProjectSidebarProps> = ({
  currentProject,
  projectMap,
  activeFilePath,
  onSelectFile,
  onOpenProject,
  onTriggerIndex,
  isIndexing,
}) => {
  const [activeTab, setActiveTab] = useState<'files' | 'map' | 'route'>('files');
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({ '': true, src: true });
  const [lazyChildrenMap, setLazyChildrenMap] = useState<Record<string, FileNode[]>>({});
  const [loadingDirs, setLoadingDirs] = useState<Record<string, boolean>>({});
  const [fileFilter, setFileFilter] = useState('');

  const [mapSubView, setMapSubView] = useState<MapSubView>('modules');
  const [showHeuristic, setShowHeuristic] = useState<boolean>(true);
  const [stepStatuses, setStepStatuses] = useState<Record<string, ReadingStepStatus>>({});

  // Sync stepStatuses when projectMap loads
  useEffect(() => {
    if (!projectMap?.reading_route) return;
    const next: Record<string, ReadingStepStatus> = {};
    for (const s of projectMap.reading_route) {
      const key = s.id || s.file_path || String(s.step);
      if (s.status === 'done' || s.status === 'reading' || s.status === 'skipped' || s.status === 'unread') {
        next[key] = s.status;
      } else if (s.completed) {
        next[key] = 'done';
      } else {
        next[key] = 'unread';
      }
    }
    setStepStatuses(next);

    // Auto-select ROS2 or LLM App subview if detected
    const domains = projectMap.detected_domains || [];
    if (domains.includes('ros2') && mapSubView === 'modules') {
      setMapSubView('ros2');
    } else if (domains.includes('llm_app') && mapSubView === 'modules') {
      setMapSubView('llm_app');
    }
  }, [projectMap]);

  // Reset lazy directory cache when project changes
  useEffect(() => {
    setLazyChildrenMap({});
    setLoadingDirs({});
  }, [currentProject?.absolute_path]);

  const toggleFolder = async (node: FileNode) => {
    const path = node.path;
    const willExpand = !expandedFolders[path];
    setExpandedFolders((prev) => ({ ...prev, [path]: willExpand }));

    if (
      willExpand &&
      node.is_dir &&
      (!node.children || node.children.length === 0) &&
      !lazyChildrenMap[path] &&
      !loadingDirs[path]
    ) {
      setLoadingDirs((prev) => ({ ...prev, [path]: true }));
      try {
        const children = await ApiService.listProjectDir(path);
        setLazyChildrenMap((prev) => ({ ...prev, [path]: children }));
      } catch (err) {
        console.warn('Lazy load directory failed:', err);
      } finally {
        setLoadingDirs((prev) => ({ ...prev, [path]: false }));
      }
    }
  };

  const handleStepStatusChange = async (stepKey: string, nextStatus: ReadingStepStatus) => {
    setStepStatuses((prev) => ({ ...prev, [stepKey]: nextStatus }));
    try {
      await ApiService.setReadingStepStatus(stepKey, nextStatus);
    } catch (err) {
      console.warn('Failed to persist reading step status:', err);
    }
  };

  const matchesFilter = (node: FileNode, query: string): boolean => {
    if (!query) return true;
    const q = query.toLowerCase();
    if (node.name.toLowerCase().includes(q) || node.path.toLowerCase().includes(q)) {
      return true;
    }
    const children = lazyChildrenMap[node.path] || node.children || [];
    return children.some((c) => matchesFilter(c, q));
  };

  const renderFileNode = (node: FileNode, level = 0) => {
    if (fileFilter && !matchesFilter(node, fileFilter)) {
      return null;
    }

    const isExpanded = Boolean(expandedFolders[node.path]) || Boolean(fileFilter);
    const isCurrent = node.path === activeFilePath || node.absolute_path === activeFilePath;
    const effectiveChildren =
      node.children && node.children.length > 0 ? node.children : lazyChildrenMap[node.path] || [];

    if (node.is_dir) {
      return (
        <div key={node.path || node.name} style={{ marginLeft: `${level * 12}px` }}>
          <div
            onClick={() => toggleFolder(node)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '4px 8px',
              borderRadius: '4px',
              cursor: 'pointer',
              color: '#cdd6f4',
              fontSize: '13px',
              userSelect: 'none',
              background: 'transparent',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = '#313244')}
            onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
          >
            {isExpanded ? <ChevronDown size={14} color="#a6adc8" /> : <ChevronRight size={14} color="#a6adc8" />}
            {isExpanded ? <FolderOpen size={16} color="#89b4fa" /> : <Folder size={16} color="#89b4fa" />}
            <span style={{ fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {node.name}
            </span>
            {loadingDirs[node.path] && (
              <span style={{ fontSize: '10px', color: '#6c7086', marginLeft: 'auto' }}>加载中...</span>
            )}
          </div>

          {isExpanded && effectiveChildren.length > 0 && (
            <div>{effectiveChildren.map((child) => renderFileNode(child, level + 1))}</div>
          )}
        </div>
      );
    }

    return (
      <div
        key={node.path}
        onClick={() => onSelectFile(node.path)}
        style={{
          marginLeft: `${level * 12 + 14}px`,
          display: 'flex',
          alignItems: 'center',
          gap: '6px',
          padding: '4px 8px',
          borderRadius: '4px',
          cursor: 'pointer',
          color: isCurrent ? '#89b4fa' : '#a6adc8',
          background: isCurrent ? 'rgba(137, 180, 250, 0.15)' : 'transparent',
          borderLeft: isCurrent ? '2px solid #89b4fa' : '2px solid transparent',
          fontSize: '12.5px',
          userSelect: 'none',
        }}
        onMouseEnter={(e) => {
          if (!isCurrent) e.currentTarget.style.background = '#313244';
        }}
        onMouseLeave={(e) => {
          if (!isCurrent) e.currentTarget.style.background = 'transparent';
        }}
      >
        {node.language === 'markdown' ? (
          <FileText size={15} color="#fab387" />
        ) : (
          <FileCode size={15} color="#a6e3a1" />
        )}
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{node.name}</span>
      </div>
    );
  };

  // Filtered entities and edges for Project Map subviews
  const ros2Entities = useMemo(
    () => (projectMap?.entities || []).filter((e) => e.domain === 'ros2'),
    [projectMap]
  );
  const llmEntities = useMemo(
    () => (projectMap?.entities || []).filter((e) => e.domain === 'llm_app'),
    [projectMap]
  );
  const filteredEdges = useMemo(
    () =>
      (projectMap?.edges || []).filter((e) =>
        showHeuristic ? true : e.confidence === 'exact'
      ),
    [projectMap, showHeuristic]
  );

  const routeProgressStats = useMemo(() => {
    const steps = projectMap?.reading_route || [];
    const total = steps.length;
    let doneCount = 0;
    let readingCount = 0;
    for (const s of steps) {
      const k = s.id || s.file_path || String(s.step);
      const st = stepStatuses[k] || s.status || 'unread';
      if (st === 'done') doneCount++;
      else if (st === 'reading') readingCount++;
    }
    return { total, doneCount, readingCount };
  }, [projectMap, stepStatuses]);

  const renderConfidenceBadge = (conf?: string) => {
    const isHeuristic = conf === 'heuristic';
    return (
      <span
        style={{
          fontSize: '9.5px',
          padding: '1px 5px',
          borderRadius: '3px',
          border: isHeuristic ? '1px dashed #f9e2af' : '1px solid rgba(166, 227, 161, 0.3)',
          color: isHeuristic ? '#f9e2af' : '#a6e3a1',
          background: isHeuristic ? 'rgba(249, 226, 175, 0.08)' : 'rgba(166, 227, 161, 0.08)',
        }}
      >
        {isHeuristic ? '启发式推断' : '精确解析'}
      </span>
    );
  };

  const renderEntityCard = (ent: GraphEntity, accentColor: string) => {
    let meta: Record<string, any> = {};
    try {
      meta = JSON.parse(ent.metadata_json || '{}');
    } catch {
      meta = {};
    }
    const conf = meta.confidence || 'exact';
    if (!showHeuristic && conf === 'heuristic') return null;

    return (
      <div
        key={ent.id}
        onClick={() => onSelectFile(ent.file_path, ent.line_start)}
        style={{
          padding: '7px 9px',
          background: '#1e1e2e',
          border: conf === 'heuristic' ? '1px dashed #45475a' : '1px solid #313244',
          borderLeft: `3px solid ${accentColor}`,
          borderRadius: '5px',
          cursor: 'pointer',
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '6px' }}>
          <span style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4', wordBreak: 'break-all' }}>
            {ent.name}
          </span>
          {renderConfidenceBadge(conf)}
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: '4px', fontSize: '10.5px', color: '#6c7086' }}>
          <span>
            {ent.file_path}:{ent.line_start}
          </span>
          <span style={{ color: accentColor }}>{ent.kind}</span>
        </div>
      </div>
    );
  };

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        background: '#181825',
        borderRight: '1px solid #313244',
      }}
    >
      {/* Top Header / Project Info */}
      <div
        style={{
          padding: '12px 14px',
          borderBottom: '1px solid #313244',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <div style={{ overflow: 'hidden' }}>
          <div style={{ fontSize: '11px', color: '#6c7086', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
            项目工作区
          </div>
          <div
            style={{
              fontSize: '14px',
              fontWeight: 600,
              color: '#cdd6f4',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            }}
          >
            {currentProject ? currentProject.name : '未打开项目'}
          </div>
        </div>

        <button
          onClick={onOpenProject}
          title="选择并导入文件夹"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '4px',
            padding: '5px 8px',
            background: '#313244',
            color: '#cdd6f4',
            border: '1px solid rgba(255,255,255,0.08)',
            borderRadius: '5px',
            fontSize: '11px',
            cursor: 'pointer',
          }}
        >
          <FolderPlus size={14} color="#89b4fa" />
          导入
        </button>
      </div>

      {/* Main Sidebar Tabs */}
      <div
        style={{
          display: 'flex',
          background: '#11111b',
          borderBottom: '1px solid #313244',
        }}
      >
        {[
          { id: 'files', label: '文件树', icon: Folder },
          { id: 'map', label: '项目地图', icon: Map },
          { id: 'route', label: '阅读路线', icon: Compass },
        ].map((tab) => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as any)}
              style={{
                flex: 1,
                padding: '8px 0',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '5px',
                background: isActive ? '#181825' : 'transparent',
                color: isActive ? '#89b4fa' : '#6c7086',
                border: 'none',
                borderBottom: isActive ? '2px solid #89b4fa' : '2px solid transparent',
                fontSize: '12px',
                fontWeight: isActive ? 600 : 400,
                cursor: 'pointer',
              }}
            >
              <Icon size={14} />
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Tab Content */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '10px 8px' }}>
        {activeTab === 'files' && (
          <div>
            {currentProject?.children && (
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                  background: '#11111b',
                  border: '1px solid #313244',
                  borderRadius: '5px',
                  padding: '4px 8px',
                  marginBottom: '8px',
                }}
              >
                <Search size={13} color="#6c7086" />
                <input
                  type="text"
                  value={fileFilter}
                  onChange={(e) => setFileFilter(e.target.value)}
                  placeholder="过滤文件名或路径..."
                  style={{
                    flex: 1,
                    background: 'transparent',
                    border: 'none',
                    outline: 'none',
                    color: '#cdd6f4',
                    fontSize: '11.5px',
                  }}
                />
              </div>
            )}

            {currentProject?.children ? (
              currentProject.children.map((child) => renderFileNode(child, 0))
            ) : (
              <div style={{ padding: '24px 12px', textAlign: 'center', color: '#6c7086', fontSize: '13px' }}>
                <Folder size={32} style={{ margin: '0 auto 10px', opacity: 0.5 }} />
                尚未导入任何项目目录。<br />
                点击上方「导入」即可开始。
              </div>
            )}
          </div>
        )}

        {activeTab === 'map' && (
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '10px' }}>
              <span style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4' }}>
                代码与领域拓扑图
              </span>
              <button
                onClick={onTriggerIndex}
                disabled={isIndexing}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '4px',
                  padding: '4px 8px',
                  background: isIndexing ? '#45475a' : '#89b4fa',
                  color: isIndexing ? '#a6adc8' : '#11111b',
                  border: 'none',
                  borderRadius: '4px',
                  fontSize: '11px',
                  fontWeight: 600,
                  cursor: isIndexing ? 'default' : 'pointer',
                }}
              >
                <RefreshCw size={12} className={isIndexing ? 'spin' : ''} />
                {isIndexing ? '增量索引中...' : '增量更新索引'}
              </button>
            </div>

            {projectMap ? (
              <div>
                {/* Summary KPI Cards */}
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '1fr 1fr 1fr',
                    gap: '6px',
                    marginBottom: '10px',
                  }}
                >
                  <div style={{ background: '#313244', padding: '6px 8px', borderRadius: '6px' }}>
                    <div style={{ fontSize: '10px', color: '#a6adc8' }}>符号总数</div>
                    <div style={{ fontSize: '15px', fontWeight: 'bold', color: '#89b4fa' }}>
                      {projectMap.total_symbols}
                    </div>
                  </div>
                  <div style={{ background: '#313244', padding: '6px 8px', borderRadius: '6px' }}>
                    <div style={{ fontSize: '10px', color: '#a6adc8' }}>领域实体</div>
                    <div style={{ fontSize: '15px', fontWeight: 'bold', color: '#cba6f7' }}>
                      {projectMap.entities?.length || 0}
                    </div>
                  </div>
                  <div style={{ background: '#313244', padding: '6px 8px', borderRadius: '6px' }}>
                    <div style={{ fontSize: '10px', color: '#a6adc8' }}>关系边数</div>
                    <div style={{ fontSize: '15px', fontWeight: 'bold', color: '#a6e3a1' }}>
                      {projectMap.edges?.length || 0}
                    </div>
                  </div>
                </div>

                {/* Detected Domain Badges & Confidence Filter */}
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    marginBottom: '10px',
                    flexWrap: 'wrap',
                    gap: '6px',
                  }}
                >
                  <div style={{ display: 'flex', gap: '4px', alignItems: 'center' }}>
                    {(projectMap.detected_domains || []).map((dom) => (
                      <span
                        key={dom}
                        style={{
                          fontSize: '10px',
                          padding: '2px 6px',
                          borderRadius: '4px',
                          fontWeight: 600,
                          background: dom === 'ros2' ? 'rgba(137, 180, 250, 0.2)' : 'rgba(203, 166, 247, 0.2)',
                          color: dom === 'ros2' ? '#89b4fa' : '#cba6f7',
                        }}
                      >
                        {dom === 'ros2' ? '🤖 ROS2 工程' : '🧠 LLM 应用'}
                      </span>
                    ))}
                  </div>
                  <label
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      fontSize: '10.5px',
                      color: '#a6adc8',
                      cursor: 'pointer',
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={showHeuristic}
                      onChange={(e) => setShowHeuristic(e.target.checked)}
                    />
                    含启发式推断
                  </label>
                </div>

                {/* 4-View Switcher */}
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(4, 1fr)',
                    gap: '4px',
                    background: '#11111b',
                    padding: '4px',
                    borderRadius: '6px',
                    marginBottom: '12px',
                  }}
                >
                  {[
                    { id: 'modules', label: '模块', icon: Layers },
                    { id: 'calls', label: '调用链', icon: GitBranch },
                    { id: 'ros2', label: `ROS2 (${ros2Entities.length})`, icon: Cpu },
                    { id: 'llm_app', label: `LLM (${llmEntities.length})`, icon: Bot },
                  ].map((v) => {
                    const VIcon = v.icon;
                    const active = mapSubView === v.id;
                    return (
                      <button
                        key={v.id}
                        onClick={() => setMapSubView(v.id as MapSubView)}
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          gap: '3px',
                          padding: '5px 2px',
                          fontSize: '10.5px',
                          fontWeight: active ? 600 : 400,
                          borderRadius: '4px',
                          border: 'none',
                          cursor: 'pointer',
                          background: active ? '#313244' : 'transparent',
                          color: active ? '#cdd6f4' : '#6c7086',
                        }}
                      >
                        <VIcon size={11} />
                        {v.label}
                      </button>
                    );
                  })}
                </div>

                {/* Subview 1: Modules & Entrypoints */}
                {mapSubView === 'modules' && (
                  <div>
                    <div style={{ fontSize: '11px', fontWeight: 600, color: '#cdd6f4', marginBottom: '6px' }}>
                      🚀 识别系统入口 ({projectMap.entry_points.length})
                    </div>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', marginBottom: '14px' }}>
                      {projectMap.entry_points.slice(0, 12).map((ep) => (
                        <div
                          key={ep}
                          onClick={() => onSelectFile(ep)}
                          style={{
                            padding: '6px 8px',
                            background: '#313244',
                            borderLeft: '3px solid #f9e2af',
                            borderRadius: '4px',
                            fontSize: '11.5px',
                            color: '#f9e2af',
                            cursor: 'pointer',
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                          }}
                        >
                          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ep}</span>
                          <ArrowRight size={13} />
                        </div>
                      ))}
                    </div>

                    <div style={{ fontSize: '11px', fontWeight: 600, color: '#cdd6f4', marginBottom: '6px' }}>
                      📦 代码包与模块 ({projectMap.modules.length})
                    </div>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                      {projectMap.modules.slice(0, 25).map((mod) => (
                        <div
                          key={mod.id}
                          onClick={() => onSelectFile(mod.path)}
                          style={{
                            padding: '8px',
                            background: '#1e1e2e',
                            border: '1px solid #313244',
                            borderRadius: '6px',
                            cursor: 'pointer',
                          }}
                        >
                          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <span style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4' }}>{mod.name}</span>
                            <span
                              style={{
                                fontSize: '10px',
                                background: '#313244',
                                color: mod.kind === 'ros2_package' ? '#89b4fa' : '#a6adc8',
                                padding: '2px 5px',
                                borderRadius: '3px',
                              }}
                            >
                              {mod.kind === 'ros2_package' ? 'ROS2 Pkg' : `${mod.symbols_count} 符号`}
                            </span>
                          </div>
                          <div style={{ fontSize: '10.5px', color: '#6c7086', marginTop: '3px' }}>{mod.path}</div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Subview 2: Call & Inheritance Graph */}
                {mapSubView === 'calls' && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                    <div style={{ fontSize: '11px', fontWeight: 600, color: '#cdd6f4', marginBottom: '2px' }}>
                      🔗 调用与继承边 ({filteredEdges.filter((e) => e.kind === 'calls' || e.kind === 'inherits').length})
                    </div>
                    {filteredEdges
                      .filter((e) => e.kind === 'calls' || e.kind === 'inherits')
                      .slice(0, 40)
                      .map((edge: GraphEdge) => (
                        <div
                          key={edge.id}
                          onClick={() => onSelectFile(edge.file_path, edge.line)}
                          style={{
                            padding: '7px 8px',
                            background: '#1e1e2e',
                            border: edge.confidence === 'heuristic' ? '1px dashed #45475a' : '1px solid #313244',
                            borderRadius: '5px',
                            cursor: 'pointer',
                            fontSize: '11.5px',
                          }}
                        >
                          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <span style={{ color: '#89b4fa', fontWeight: 600 }}>
                              {edge.kind === 'inherits' ? '继承' : '调用'} → {edge.dst_id.split(':').pop()}
                            </span>
                            {renderConfidenceBadge(edge.confidence)}
                          </div>
                          <div style={{ fontSize: '10.5px', color: '#6c7086', marginTop: '3px' }}>
                            {edge.file_path}:{edge.line}
                          </div>
                        </div>
                      ))}
                  </div>
                )}

                {/* Subview 3: ROS2 Node / Topic / Service / Launch Topology */}
                {mapSubView === 'ros2' && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                    {ros2Entities.length === 0 ? (
                      <div style={{ padding: '16px 8px', textAlign: 'center', color: '#6c7086', fontSize: '12px' }}>
                        当前工作区未检测到 ROS2 节点或接口定义。
                      </div>
                    ) : (
                      <>
                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#89b4fa', marginBottom: '5px' }}>
                            🤖 ROS2 节点 (Nodes: {ros2Entities.filter((e) => e.kind === 'ros2_node').length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {ros2Entities
                              .filter((e) => e.kind === 'ros2_node')
                              .map((e) => renderEntityCard(e, '#89b4fa'))}
                          </div>
                        </div>

                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#a6e3a1', marginBottom: '5px' }}>
                            📡 话题与服务 (Topics & Services:{' '}
                            {ros2Entities.filter((e) => e.kind === 'ros2_topic' || e.kind === 'ros2_service').length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {ros2Entities
                              .filter((e) => e.kind === 'ros2_topic' || e.kind === 'ros2_service')
                              .map((e) => renderEntityCard(e, '#a6e3a1'))}
                          </div>
                        </div>

                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#f9e2af', marginBottom: '5px' }}>
                            🚀 Launch 启动文件与消息接口 (Launches & Interfaces:{' '}
                            {ros2Entities.filter((e) => e.kind === 'ros2_launch' || e.kind === 'ros2_interface').length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {ros2Entities
                              .filter((e) => e.kind === 'ros2_launch' || e.kind === 'ros2_interface')
                              .map((e) => renderEntityCard(e, '#f9e2af'))}
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                )}

                {/* Subview 4: LLM App Flow */}
                {mapSubView === 'llm_app' && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
                    {llmEntities.length === 0 ? (
                      <div style={{ padding: '16px 8px', textAlign: 'center', color: '#6c7086', fontSize: '12px' }}>
                        当前工作区未检测到 LLM 客户端、Prompt 或 Agent 编排。
                      </div>
                    ) : (
                      <>
                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#cba6f7', marginBottom: '5px' }}>
                            🔌 LLM 客户端与调用点 ({llmEntities.filter((e) => e.kind === 'llm_client' || e.kind === 'llm_call').length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {llmEntities
                              .filter((e) => e.kind === 'llm_client' || e.kind === 'llm_call')
                              .map((e) => renderEntityCard(e, '#cba6f7'))}
                          </div>
                        </div>

                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#f9e2af', marginBottom: '5px' }}>
                            📝 Prompt 模板与工具定义 ({llmEntities.filter((e) => e.kind === 'prompt_template' || e.kind === 'tool_def').length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {llmEntities
                              .filter((e) => e.kind === 'prompt_template' || e.kind === 'tool_def')
                              .map((e) => renderEntityCard(e, '#f9e2af'))}
                          </div>
                        </div>

                        <div>
                          <div style={{ fontSize: '11px', fontWeight: 600, color: '#89b4fa', marginBottom: '5px' }}>
                            🔄 Agent 编排 / 入口 / 配置项 (
                            {llmEntities.filter((e) => ['agent_workflow', 'llm_entrypoint', 'config_key', 'rag_pipeline'].includes(e.kind)).length})
                          </div>
                          <div style={{ display: 'flex', flexDirection: 'column', gap: '5px' }}>
                            {llmEntities
                              .filter((e) =>
                                ['agent_workflow', 'llm_entrypoint', 'config_key', 'rag_pipeline'].includes(e.kind)
                              )
                              .map((e) => renderEntityCard(e, '#89b4fa'))}
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '24px 8px', color: '#6c7086', fontSize: '12px' }}>
                尚未计算项目拓扑。<br />
                点击上方「增量更新索引」即可生成。
              </div>
            )}
          </div>
        )}

        {activeTab === 'route' && (
          <div>
            <div style={{ marginBottom: '10px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4' }}>
                  4 阶段结构化阅读路线
                </span>
                {routeProgressStats.total > 0 && (
                  <span style={{ fontSize: '11px', color: '#a6e3a1', fontWeight: 600 }}>
                    已掌握 {routeProgressStats.doneCount} / {routeProgressStats.total}
                  </span>
                )}
              </div>
              {routeProgressStats.total > 0 && (
                <div
                  style={{
                    width: '100%',
                    height: '5px',
                    background: '#313244',
                    borderRadius: '3px',
                    marginTop: '6px',
                    overflow: 'hidden',
                  }}
                >
                  <div
                    style={{
                      width: `${Math.round((routeProgressStats.doneCount / routeProgressStats.total) * 100)}%`,
                      height: '100%',
                      background: '#a6e3a1',
                      transition: 'width 0.25s ease',
                    }}
                  />
                </div>
              )}
            </div>

            {projectMap?.reading_route && projectMap.reading_route.length > 0 ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {projectMap.reading_route.map((item) => {
                  const stepKey = item.id || item.file_path || String(item.step);
                  const status: ReadingStepStatus =
                    (stepStatuses[stepKey] as ReadingStepStatus) ||
                    (item.status as ReadingStepStatus) ||
                    'unread';
                  const isDone = status === 'done';
                  const isReading = status === 'reading';
                  const stageMeta = STAGE_LABELS[item.stage || ''] || {
                    title: item.priority === 1 ? '阶段 1 · 入口启动' : '核心下钻',
                    color: item.priority === 1 ? '#f9e2af' : '#89b4fa',
                    bg: item.priority === 1 ? 'rgba(249, 226, 175, 0.14)' : 'rgba(137, 180, 250, 0.14)',
                  };

                  return (
                    <div
                      key={stepKey}
                      style={{
                        padding: '10px',
                        background: isDone
                          ? 'rgba(166, 227, 161, 0.06)'
                          : isReading
                          ? 'rgba(137, 180, 250, 0.08)'
                          : '#1e1e2e',
                        border: isDone
                          ? '1px solid rgba(166, 227, 161, 0.25)'
                          : isReading
                          ? '1px solid rgba(137, 180, 250, 0.35)'
                          : '1px solid #313244',
                        borderRadius: '6px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '6px' }}>
                        <span style={{ fontSize: '11px', fontWeight: 'bold', color: '#89b4fa' }}>
                          #{item.step}
                        </span>
                        <span
                          style={{
                            fontSize: '9.5px',
                            padding: '2px 6px',
                            borderRadius: '3px',
                            background: stageMeta.bg,
                            color: stageMeta.color,
                          }}
                        >
                          {stageMeta.title}
                        </span>
                      </div>

                      <div
                        onClick={() => onSelectFile(item.file_path, item.line_start)}
                        style={{
                          fontSize: '12.5px',
                          fontWeight: 600,
                          color: '#cdd6f4',
                          marginTop: '6px',
                          cursor: 'pointer',
                        }}
                      >
                        {item.title}
                      </div>

                      <div style={{ fontSize: '11px', color: '#a6adc8', marginTop: '4px', lineHeight: '1.45' }}>
                        {item.rationale}
                      </div>

                      <div
                        style={{
                          display: 'flex',
                          justifyContent: 'space-between',
                          alignItems: 'center',
                          marginTop: '8px',
                          fontSize: '10px',
                          color: '#6c7086',
                        }}
                      >
                        <span>
                          {item.file_path}
                          {item.line_start ? `:L${item.line_start}` : ''}
                        </span>
                        <span>{item.symbol_count} 符号</span>
                      </div>

                      {/* Persistent Reading Status Selector */}
                      <div
                        style={{
                          display: 'flex',
                          gap: '4px',
                          marginTop: '8px',
                          paddingTop: '6px',
                          borderTop: '1px solid rgba(255,255,255,0.05)',
                        }}
                      >
                        {[
                          { id: 'unread', label: '未读', icon: Circle, color: '#6c7086' },
                          { id: 'reading', label: '阅读中', icon: Clock, color: '#89b4fa' },
                          { id: 'done', label: '已掌握', icon: CheckCircle, color: '#a6e3a1' },
                          { id: 'skipped', label: '跳过', icon: SkipForward, color: '#f9e2af' },
                        ].map((opt) => {
                          const SIcon = opt.icon;
                          const active = status === opt.id;
                          return (
                            <button
                              key={opt.id}
                              onClick={() => handleStepStatusChange(stepKey, opt.id as ReadingStepStatus)}
                              style={{
                                flex: 1,
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center',
                                gap: '3px',
                                padding: '3px 4px',
                                borderRadius: '4px',
                                border: active ? `1px solid ${opt.color}` : '1px solid transparent',
                                background: active ? 'rgba(255,255,255,0.06)' : 'transparent',
                                color: active ? opt.color : '#6c7086',
                                fontSize: '10px',
                                cursor: 'pointer',
                              }}
                            >
                              <SIcon size={11} />
                              {opt.label}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '24px 8px', color: '#6c7086', fontSize: '12px' }}>
                请先在「项目地图」中触发索引以生成 4 阶段阅读路线。
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
