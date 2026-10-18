import React, { useState } from 'react';
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
} from 'lucide-react';
import { FileNode, ProjectMap } from '../types';

interface ProjectSidebarProps {
  currentProject: FileNode | null;
  projectMap: ProjectMap | null;
  activeFilePath: string;
  onSelectFile: (filePath: string) => void;
  onOpenProject: () => void;
  onTriggerIndex: () => void;
  isIndexing: boolean;
}

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
  const [completedSteps, setCompletedSteps] = useState<Record<number, boolean>>({});

  const toggleFolder = (path: string) => {
    setExpandedFolders((prev) => ({ ...prev, [path]: !prev[path] }));
  };

  const toggleStep = (stepNumber: number) => {
    setCompletedSteps((prev) => ({ ...prev, [stepNumber]: !prev[stepNumber] }));
  };

  const renderFileNode = (node: FileNode, level = 0) => {
    const isExpanded = !!expandedFolders[node.path];
    const isCurrent = node.path === activeFilePath || node.absolute_path === activeFilePath;

    if (node.is_dir) {
      return (
        <div key={node.path || node.name} style={{ marginLeft: `${level * 12}px` }}>
          <div
            onClick={() => toggleFolder(node.path)}
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
            <span style={{ fontWeight: 500 }}>{node.name}</span>
          </div>

          {isExpanded && node.children && (
            <div>{node.children.map((child) => renderFileNode(child, level + 1))}</div>
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
          <div style={{ fontSize: '14px', fontWeight: 600, color: '#cdd6f4', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
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

      {/* Tabs */}
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
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
              <span style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4' }}>
                代码结构拓扑图
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
                {isIndexing ? '索引分析中...' : '重新建立索引'}
              </button>
            </div>

            {projectMap ? (
              <div>
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '1fr 1fr',
                    gap: '6px',
                    marginBottom: '14px',
                  }}
                >
                  <div style={{ background: '#313244', padding: '8px', borderRadius: '6px' }}>
                    <div style={{ fontSize: '10px', color: '#a6adc8' }}>抽取符号总数</div>
                    <div style={{ fontSize: '16px', fontWeight: 'bold', color: '#89b4fa' }}>
                      {projectMap.total_symbols}
                    </div>
                  </div>
                  <div style={{ background: '#313244', padding: '8px', borderRadius: '6px' }}>
                    <div style={{ fontSize: '10px', color: '#a6adc8' }}>分析文件数</div>
                    <div style={{ fontSize: '16px', fontWeight: 'bold', color: '#a6e3a1' }}>
                      {projectMap.indexed_files_count}
                    </div>
                  </div>
                </div>

                <div style={{ fontSize: '11px', fontWeight: 600, color: '#cdd6f4', marginBottom: '6px' }}>
                  🚀 识别系统入口 ({projectMap.entry_points.length})
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', marginBottom: '14px' }}>
                  {projectMap.entry_points.map((ep) => (
                    <div
                      key={ep}
                      onClick={() => onSelectFile(ep)}
                      style={{
                        padding: '6px 8px',
                        background: '#313244',
                        borderLeft: '3px solid #f9e2af',
                        borderRadius: '4px',
                        fontSize: '12px',
                        color: '#f9e2af',
                        cursor: 'pointer',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                      }}
                    >
                      <span>{ep}</span>
                      <ArrowRight size={13} />
                    </div>
                  ))}
                </div>

                <div style={{ fontSize: '11px', fontWeight: 600, color: '#cdd6f4', marginBottom: '6px' }}>
                  📦 核心代码模块 ({projectMap.modules.length})
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                  {projectMap.modules.slice(0, 15).map((mod) => (
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
                        <span style={{ fontSize: '12.5px', fontWeight: 600, color: '#cdd6f4' }}>{mod.name}</span>
                        <span style={{ fontSize: '10px', background: '#313244', color: '#a6adc8', padding: '2px 5px', borderRadius: '3px' }}>
                          {mod.symbols_count} 符号
                        </span>
                      </div>
                      <div style={{ fontSize: '11px', color: '#6c7086', marginTop: '3px' }}>
                        {mod.path}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '24px 8px', color: '#6c7086', fontSize: '12px' }}>
                尚未计算项目拓扑。<br />
                点击上方「重新建立索引」即可生成。
              </div>
            )}
          </div>
        )}

        {activeTab === 'route' && (
          <div>
            <div style={{ marginBottom: '10px' }}>
              <div style={{ fontSize: '12px', fontWeight: 600, color: '#cdd6f4' }}>
                智能推荐阅读顺序 (基于图计算)
              </div>
              <div style={{ fontSize: '11px', color: '#6c7086', marginTop: '2px' }}>
                从核心入口出发，按依赖引用密度与核心度排序
              </div>
            </div>

            {projectMap?.reading_route && projectMap.reading_route.length > 0 ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {projectMap.reading_route.map((item) => {
                  const isDone = !!completedSteps[item.step];
                  return (
                    <div
                      key={item.step}
                      style={{
                        padding: '10px',
                        background: isDone ? 'rgba(166, 227, 161, 0.05)' : '#1e1e2e',
                        border: isDone ? '1px solid rgba(166, 227, 161, 0.2)' : '1px solid #313244',
                        borderRadius: '6px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: '6px' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          <button
                            onClick={() => toggleStep(item.step)}
                            style={{ background: 'transparent', border: 'none', cursor: 'pointer', padding: 0 }}
                          >
                            {isDone ? (
                              <CheckCircle size={16} color="#a6e3a1" />
                            ) : (
                              <Circle size={16} color="#6c7086" />
                            )}
                          </button>
                          <span style={{ fontSize: '11px', fontWeight: 'bold', color: '#89b4fa' }}>
                            第 {item.step} 步
                          </span>
                        </div>
                        <span
                          style={{
                            fontSize: '10px',
                            padding: '2px 6px',
                            borderRadius: '3px',
                            background: item.priority === 1 ? 'rgba(249, 226, 175, 0.2)' : 'rgba(137, 180, 250, 0.1)',
                            color: item.priority === 1 ? '#f9e2af' : '#89b4fa',
                          }}
                        >
                          {item.priority === 1 ? '入口启动' : '核心下钻'}
                        </span>
                      </div>

                      <div
                        onClick={() => onSelectFile(item.file_path)}
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

                      <div style={{ fontSize: '11px', color: '#a6adc8', marginTop: '4px', lineHeight: '1.4' }}>
                        {item.rationale}
                      </div>

                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '8px', fontSize: '10.5px', color: '#6c7086' }}>
                        <span>{item.file_path}</span>
                        <span>{item.symbol_count} 符号</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div style={{ textAlign: 'center', padding: '24px 8px', color: '#6c7086', fontSize: '12px' }}>
                请先在「项目地图」中触发索引以生成阅读路线。
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
