"use client";

import React from "react"

import Image from "next/image";
import { useState, useEffect } from "react";
import {
  Play,
  Pause,
  Minus,
  X,
  ChevronDown,
  User,
  LogOut,
  FolderOpen,
  RefreshCw,
  Download,
  MessageCircle,
  Globe,
  Headphones,
  BookOpen,
  Loader2,
  Check,
  AlertCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

// Background images for carousel
const backgroundImages = [
  "/images/bg-1.jpg", // Magical forest
  "/images/bg-2.jpg", // Elf character
  "/images/bg-3.jpg", // Ancient ruins
  "/images/bg-4.jpg", // Desert battle
  "/images/bg-5.jpg", // Grand city
];

const newsItems = [
  { text: "Patch Notes 1.1.91", href: "https://forum.crazy-esports.com/" },
  { text: "Patch Notes 1.1.9", href: "https://forum.crazy-esports.com/" },
  { text: "Server Maintenance", href: "https://forum.crazy-esports.com/" },
];

const promoCards = [
  {
    id: 1,
    title: "Visit our partner!",
    image: "/images/promo-partner.jpg",
    href: "https://tera-europe.net/",
  },
  {
    id: 2,
    title: "Come to our forum!",
    image: "/images/promo-forum.jpg",
    href: "https://forum.crazy-esports.com/startpage-en/",
  },
  {
    id: 3,
    title: "Join our team!",
    image: "/images/promo-team.jpg",
    href: "https://forum.crazy-esports.com/",
  },
];

const languages = ["ENGLISH", "GERMAN", "FRENCH", "RUSSIAN"] as const;

type LauncherState = "ready" | "downloading" | "paused" | "checking";
type UpdateCheckState = "idle" | "checking" | "upToDate" | "error";

export default function LauncherPage() {
  const [isLoggedIn, setIsLoggedIn] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [displayName] = useState("valkytest1");
  const [onlineCount] = useState(13);
  const [language, setLanguage] = useState<(typeof languages)[number]>("ENGLISH");
  const [launcherState, setLauncherState] = useState<LauncherState>("ready");
  const [downloadProgress, setDownloadProgress] = useState(67);
  const [downloadSpeed] = useState("12.4 MB/s");
  const [downloadedSize] = useState("33.5");
  const [totalSize] = useState("50.0");
  const [currentBgIndex, setCurrentBgIndex] = useState(0);
  
  // New states for additional features
  const [checkProgress, setCheckProgress] = useState(0);
  const [checkedFiles, setCheckedFiles] = useState(0);
  const [totalFiles] = useState(12847);
  const [updateCheckState, setUpdateCheckState] = useState<UpdateCheckState>("idle");
  const [showUpdateNotification, setShowUpdateNotification] = useState(false);
  const [gameDirectory, setGameDirectory] = useState("C:\\Games\\TERA Europe Classic");
  const [showDirectoryDialog, setShowDirectoryDialog] = useState(false);
  const [tempDirectory, setTempDirectory] = useState("");
  const fileInputRef = React.useRef<HTMLInputElement>(null);

  // Demo login handler
  const handleLogin = (e: React.FormEvent) => {
    e.preventDefault();
    if (username && password) {
      setIsLoggedIn(true);
    }
  };

  // Logout handler
  const handleLogout = () => {
    setIsLoggedIn(false);
    setUsername("");
    setPassword("");
  };

  // Background carousel - auto cycle every 8 seconds
  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentBgIndex((prev) => (prev + 1) % backgroundImages.length);
    }, 8000);
    return () => clearInterval(interval);
  }, []);

  // Toggle pause/resume during download
  const togglePause = () => {
    if (launcherState === "downloading") {
      setLauncherState("paused");
    } else if (launcherState === "paused") {
      setLauncherState("downloading");
    }
  };

  // Demo: cycle through states (including checking)
  const cycleState = () => {
    if (launcherState === "ready") {
      setLauncherState("checking");
      setCheckProgress(0);
      setCheckedFiles(0);
    } else if (launcherState === "checking") {
      setLauncherState("downloading");
      setDownloadProgress(67);
    } else if (launcherState === "downloading") {
      setLauncherState("paused");
    } else {
      setLauncherState("ready");
      setDownloadProgress(100);
    }
  };

  // Simulate file checking progress
  useEffect(() => {
    if (launcherState === "checking") {
      const interval = setInterval(() => {
        setCheckProgress((prev) => {
          const newProgress = prev + Math.random() * 3;
          if (newProgress >= 100) {
            setLauncherState("ready");
            return 100;
          }
          setCheckedFiles(Math.floor((newProgress / 100) * totalFiles));
          return newProgress;
        });
      }, 100);
      return () => clearInterval(interval);
    }
  }, [launcherState, totalFiles]);

  // Check launcher update handler
  const handleCheckUpdate = () => {
    setUpdateCheckState("checking");
    setShowUpdateNotification(true);
    // Simulate checking
    setTimeout(() => {
      setUpdateCheckState("upToDate");
      // Hide notification after 4 seconds
      setTimeout(() => {
        setShowUpdateNotification(false);
        setUpdateCheckState("idle");
      }, 4000);
    }, 2000);
  };

  // Check & Repair files handler
  const handleCheckFiles = () => {
    setLauncherState("checking");
    setCheckProgress(0);
    setCheckedFiles(0);
  };

  // Game directory handlers
  const openDirectoryDialog = () => {
    setTempDirectory(gameDirectory);
    setShowDirectoryDialog(true);
  };

  const saveDirectory = () => {
    if (tempDirectory.trim()) {
      setGameDirectory(tempDirectory);
    }
    setShowDirectoryDialog(false);
  };

  // Handle folder selection via hidden input
  const handleBrowseClick = () => {
    fileInputRef.current?.click();
  };

  const handleFolderSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      // Get the path from the first file's webkitRelativePath
      const path = files[0].webkitRelativePath;
      if (path) {
        // Extract the folder path
        const folderPath = path.split("/")[0];
        setTempDirectory(folderPath);
      }
    }
  };

  return (
    <div className="relative h-screen w-screen flex flex-col bg-background overflow-hidden">
      {/* Background Carousel */}
      <div className="absolute inset-0">
        {backgroundImages.map((src, index) => (
          <div
            key={src}
            className={`absolute inset-0 transition-opacity duration-1000 ${
              index === currentBgIndex ? "opacity-100" : "opacity-0"
            }`}
          >
            <Image
              src={src || "/placeholder.svg"}
              alt=""
              fill
              className="object-cover"
              priority={index === 0}
            />
          </div>
        ))}
        {/* Gradient overlays */}
        <div className="absolute inset-0 bg-gradient-to-r from-background/95 via-background/60 to-background/30" />
        <div className="absolute inset-0 bg-gradient-to-t from-background via-transparent to-background/40" />
      </div>

      {/* Content */}
      <div className="relative z-10 flex flex-col h-full">
        {/* Header */}
        <header className="flex items-center justify-between px-5 h-12 bg-background/50 backdrop-blur-sm border-b border-border/10">
          {/* Left: CES Logo + Nav Links */}
          <div className="flex items-center gap-6">
            <Image
              src="/images/ces-logo.png"
              alt="CES"
              width={60}
              height={24}
              className="h-5 w-auto opacity-90"
            />
            
            <nav className="flex items-center gap-1">
              <a href="https://discord.gg/teraeurope" target="_blank" rel="noopener noreferrer">
                <Button variant="ghost" size="sm" className="h-8 px-3 text-xs gap-1.5 text-muted-foreground hover:text-foreground">
                  <MessageCircle className="h-3.5 w-3.5" />
                  Discord
                </Button>
              </a>
              <a href="https://forum.crazy-esports.com/startpage-en/" target="_blank" rel="noopener noreferrer">
                <Button variant="ghost" size="sm" className="h-8 px-3 text-xs gap-1.5 text-muted-foreground hover:text-foreground">
                  <BookOpen className="h-3.5 w-3.5" />
                  Forum
                </Button>
              </a>
              <a href="https://forum.crazy-esports.com/support/" target="_blank" rel="noopener noreferrer">
                <Button variant="ghost" size="sm" className="h-8 px-3 text-xs gap-1.5 text-muted-foreground hover:text-foreground">
                  <Headphones className="h-3.5 w-3.5" />
                  Support
                </Button>
              </a>
              <a href="https://tera-europe-classic.com/" target="_blank" rel="noopener noreferrer">
                <Button variant="ghost" size="sm" className="h-8 px-3 text-xs gap-1.5 text-muted-foreground hover:text-foreground">
                  <Globe className="h-3.5 w-3.5" />
                  Website
                </Button>
              </a>
            </nav>
          </div>

          {/* Right: Region, Settings, User, Window Controls */}
          <div className="flex items-center gap-3">
            {/* Region Selector */}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors px-2 py-1 rounded hover:bg-accent/30">
                  <span className="text-muted-foreground/70">Region:</span>
                  <span className="font-medium text-foreground">{language}</span>
                  <ChevronDown className="h-3 w-3 opacity-50" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-[120px]">
                {languages.map((lang) => (
                  <DropdownMenuItem
                    key={lang}
                    onClick={() => setLanguage(lang)}
                    className={lang === language ? "bg-accent" : ""}
                  >
                    {lang}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* Settings - Only show when logged in */}
            {isLoggedIn && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button className="text-xs text-muted-foreground hover:text-foreground transition-colors px-2 py-1 rounded hover:bg-accent/30">
                    Settings
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-[200px]">
                  <DropdownMenuItem onClick={handleCheckUpdate} disabled={updateCheckState === "checking"}>
                    {updateCheckState === "checking" ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : updateCheckState === "upToDate" ? (
                      <Check className="h-4 w-4 mr-2 text-emerald-500" />
                    ) : (
                      <RefreshCw className="h-4 w-4 mr-2" />
                    )}
                    {updateCheckState === "checking"
                      ? "Checking..."
                      : updateCheckState === "upToDate"
                        ? "Launcher is up to date"
                        : "Check Launcher Update"}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={handleCheckFiles} disabled={launcherState === "checking"}>
                    {launcherState === "checking" ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : (
                      <Download className="h-4 w-4 mr-2" />
                    )}
                    Check & Repair Files
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={openDirectoryDialog}>
                    <FolderOpen className="h-4 w-4 mr-2" />
                    Game Directory
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    className="text-destructive focus:text-destructive"
                    onClick={handleLogout}
                  >
                    <LogOut className="h-4 w-4 mr-2" />
                    Logout
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            {/* Divider */}
            <div className="h-4 w-px bg-border/30" />

            {/* User Profile or Login */}
            {isLoggedIn ? (
              <div className="flex items-center gap-2 px-2 py-1">
                <User className="h-3.5 w-3.5 text-muted-foreground" />
                <span className="text-xs font-medium text-foreground">{displayName}</span>
              </div>
            ) : (
              <form onSubmit={handleLogin} className="flex items-center gap-2">
                <input
                  type="text"
                  placeholder="Username"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className="h-7 w-24 px-2 text-xs bg-background/50 border border-border/30 rounded focus:border-primary/50 focus:outline-none text-foreground placeholder:text-muted-foreground"
                />
                <input
                  type="password"
                  placeholder="Password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="h-7 w-24 px-2 text-xs bg-background/50 border border-border/30 rounded focus:border-primary/50 focus:outline-none text-foreground placeholder:text-muted-foreground"
                />
                <Button type="submit" size="sm" className="h-7 px-3 text-xs">
                  Login
                </Button>
                <a
                  href="https://tera-europe-classic.de/register.php?lang=de"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <Button type="button" variant="outline" size="sm" className="h-7 px-3 text-xs bg-transparent">
                    Register
                  </Button>
                </a>
              </form>
            )}

            {/* Window Controls */}
            <div className="flex items-center ml-1">
              <button
                className="p-1.5 hover:bg-accent/50 rounded transition-colors text-muted-foreground hover:text-foreground"
                aria-label="Minimize"
              >
                <Minus className="h-3.5 w-3.5" />
              </button>
              <button
                className="p-1.5 hover:bg-red-500/80 rounded transition-colors text-muted-foreground hover:text-white"
                aria-label="Close"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        </header>

        {/* Main Content */}
        <div className="flex-1 flex flex-col">
          {/* Logo & Info Section */}
          <div className="flex-1 flex items-start p-8">
            <div className="space-y-6 max-w-md">
              {/* TERA Logo */}
              <Image
                src="/images/tera-logo-clean.png"
                alt="TERA Europe Classic"
                width={300}
                height={150}
                className="w-auto h-28 object-contain drop-shadow-2xl"
                priority
              />

              {/* Game Description */}
              <p className="text-sm text-muted-foreground/90 leading-relaxed">
                TERA is a fast-paced action MMORPG set in a breathtaking world conjured by two dreaming primal gods. Take your place as a soldier for the Valkyon Federation and fight alongside other races to forge a new future.
              </p>

              {/* Player Count */}
              <button
                onClick={cycleState}
                className="flex items-center gap-3 bg-background/30 backdrop-blur-sm rounded-lg px-4 py-2.5 border border-border/20 hover:border-primary/30 transition-all w-fit group"
                title="Click to demo states"
              >
                <span className="relative flex h-2 w-2">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500" />
                </span>
                <span className="text-lg font-bold text-foreground tabular-nums group-hover:text-primary transition-colors">{onlineCount}</span>
                <span className="text-xs text-muted-foreground">players online</span>
              </button>
            </div>
          </div>

          {/* Bottom Section - Full Width */}
          <div className="mt-auto">
            {/* News Bar */}
            <div className="bg-background/30 backdrop-blur-sm border-y border-border/10 py-2.5 px-6">
              <div className="flex items-center gap-6">
                <span className="text-[10px] font-bold text-primary uppercase tracking-widest flex items-center gap-1.5 shrink-0">
                  <span className="w-1 h-1 rounded-full bg-primary" />
                  NEWS
                </span>
                <div className="flex items-center gap-6 text-xs text-muted-foreground">
                  {newsItems.map((item, i) => (
                    <a
                      key={i}
                      href={item.href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="hover:text-foreground transition-colors"
                    >
                      {item.text}
                    </a>
                  ))}
                </div>
              </div>
            </div>

            {/* Footer - Cards & Launch */}
            <footer className="bg-background/60 backdrop-blur-md border-t border-border/10 p-4">
              <div className="flex items-stretch gap-4">
                {/* Promo Cards - Takes remaining space */}
                <div className="flex-1 grid grid-cols-3 gap-3">
                  {promoCards.map((card) => (
                    <a
                      key={card.id}
                      href={card.href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="relative group overflow-hidden rounded-lg aspect-[2.2/1] border border-border/20 hover:border-primary/40 transition-all"
                    >
                      <Image
                        src={card.image || "/placeholder.svg"}
                        alt={card.title}
                        fill
                        className="object-cover group-hover:scale-105 transition-transform duration-500"
                      />
                      <div className="absolute inset-0 bg-gradient-to-t from-background/90 via-background/30 to-transparent" />
                      <div className="absolute bottom-0 left-0 right-0 p-2.5">
                        <span className="text-xs font-medium text-foreground group-hover:text-primary transition-colors">
                          {card.title}
                        </span>
                      </div>
                    </a>
                  ))}
                </div>

                {/* Launch Section - Fixed Width & Height */}
                <div className="w-64 flex flex-col bg-background/40 rounded-lg p-4 border border-border/20">
                  {/* Status - Fixed height container */}
                  <div className="h-[42px] mb-3">
                    {!isLoggedIn ? (
                      <div className="flex items-center gap-2 h-full">
                        <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
                        <span className="text-sm text-amber-400 font-medium">Login required</span>
                      </div>
                    ) : launcherState === "ready" ? (
                      <div className="flex items-center gap-2 h-full">
                        <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                        <span className="text-sm text-emerald-400 font-medium">Ready to play</span>
                      </div>
                    ) : launcherState === "checking" ? (
                      <div className="flex flex-col justify-between h-full">
                        <div className="flex items-center justify-between">
                          <span className="text-xs font-medium text-foreground flex items-center gap-1.5">
                            <Loader2 className="h-3 w-3 animate-spin" />
                            Checking files...
                          </span>
                          <span className="text-xs text-muted-foreground tabular-nums">
                            {Math.round(checkProgress)}%
                          </span>
                        </div>
                        <Progress value={checkProgress} className="h-1.5" />
                        <div className="flex justify-between text-[10px] text-muted-foreground">
                          <span>{checkedFiles.toLocaleString()} / {totalFiles.toLocaleString()} files</span>
                        </div>
                      </div>
                    ) : (
                      <div className="flex flex-col justify-between h-full">
                        <div className="flex items-center justify-between">
                          <span className="text-xs font-medium text-foreground">
                            {launcherState === "downloading" ? "Downloading..." : "Paused"}
                          </span>
                          <span className="text-xs text-muted-foreground tabular-nums">
                            {downloadProgress}%
                          </span>
                        </div>
                        <Progress value={downloadProgress} className="h-1.5" />
                        <div className="flex justify-between text-[10px] text-muted-foreground">
                          <span>{downloadedSize} GB / {totalSize} GB</span>
                          {launcherState === "downloading" && <span>{downloadSpeed}</span>}
                        </div>
                      </div>
                    )}
                  </div>

                  {/* Buttons */}
                  <div className="flex gap-2">
                    {!isLoggedIn ? (
                      <Button
                        size="sm"
                        disabled
                        className="flex-1 h-9 font-bold text-xs bg-muted text-muted-foreground cursor-not-allowed"
                      >
                        <Play className="h-3.5 w-3.5 mr-1.5 fill-current" />
                        LAUNCH
                      </Button>
                    ) : launcherState === "checking" ? (
                      <Button
                        size="sm"
                        disabled
                        className="flex-1 h-9 font-bold text-xs bg-muted text-muted-foreground cursor-not-allowed"
                      >
                        <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
                        CHECKING
                      </Button>
                    ) : launcherState !== "ready" ? (
                      <>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={togglePause}
                          className="w-12 h-9 bg-background/50 hover:bg-background/70 p-0"
                        >
                          {launcherState === "downloading" ? (
                            <Pause className="h-4 w-4" />
                          ) : (
                            <Play className="h-4 w-4" />
                          )}
                        </Button>
                        <Button
                          size="sm"
                          disabled
                          className="flex-1 h-9 font-bold text-xs bg-muted text-muted-foreground cursor-not-allowed"
                        >
                          <Play className="h-3.5 w-3.5 mr-1.5 fill-current" />
                          LAUNCH
                        </Button>
                      </>
                    ) : (
                      <Button
                        size="sm"
                        className="flex-1 h-9 font-bold text-xs bg-primary hover:bg-primary/90 text-primary-foreground shadow-lg shadow-primary/25 hover:shadow-primary/40"
                      >
                        <Play className="h-3.5 w-3.5 mr-1.5 fill-current" />
                        LAUNCH
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            </footer>
          </div>
        </div>
      </div>

      {/* Update Check Notification */}
      {showUpdateNotification && (
        <div className="fixed top-16 right-4 z-50 animate-in slide-in-from-top-2 fade-in duration-300">
          <div className="bg-background/95 backdrop-blur-sm border border-border rounded-lg shadow-xl p-4 min-w-[280px]">
            <div className="flex items-center gap-3">
              {updateCheckState === "checking" ? (
                <>
                  <Loader2 className="h-5 w-5 animate-spin text-primary" />
                  <div>
                    <p className="text-sm font-medium text-foreground">Checking for updates...</p>
                    <p className="text-xs text-muted-foreground">Please wait</p>
                  </div>
                </>
              ) : updateCheckState === "upToDate" ? (
                <>
                  <div className="h-8 w-8 rounded-full bg-emerald-500/20 flex items-center justify-center">
                    <Check className="h-4 w-4 text-emerald-500" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-foreground">Launcher is up to date</p>
                    <p className="text-xs text-muted-foreground">You have the latest version</p>
                  </div>
                </>
              ) : (
                <>
                  <div className="h-8 w-8 rounded-full bg-red-500/20 flex items-center justify-center">
                    <AlertCircle className="h-4 w-4 text-red-500" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-foreground">Update check failed</p>
                    <p className="text-xs text-muted-foreground">Please try again later</p>
                  </div>
                </>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Game Directory Dialog */}
      <Dialog open={showDirectoryDialog} onOpenChange={setShowDirectoryDialog}>
        <DialogContent className="sm:max-w-[480px] bg-background border-border">
          <DialogHeader>
            <DialogTitle>Game Directory</DialogTitle>
            <DialogDescription>
              Choose where TERA Europe Classic is installed.
            </DialogDescription>
          </DialogHeader>
          <div className="flex items-center border border-border rounded-md overflow-hidden">
            <div className="flex-1 flex items-center px-3 py-2 bg-muted/30">
              <FolderOpen className="h-4 w-4 text-muted-foreground shrink-0 mr-2" />
              <input
                type="text"
                value={tempDirectory}
                onChange={(e) => setTempDirectory(e.target.value)}
                className="flex-1 bg-transparent border-none focus:outline-none text-foreground text-sm"
              />
            </div>
            <Button
              type="button"
              variant="ghost"
              onClick={handleBrowseClick}
              className="h-full px-4 rounded-none border-l border-border hover:bg-muted/50"
            >
              Browse
            </Button>
            <input
              ref={fileInputRef}
              type="file"
              className="hidden"
              /* @ts-expect-error webkitdirectory is not standard but widely supported */
              webkitdirectory=""
              onChange={handleFolderSelect}
            />
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setShowDirectoryDialog(false)}>
              Cancel
            </Button>
            <Button onClick={saveDirectory}>
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
