/**
 * Hosting servers built from instances on this computer (Threadrinth).
 */
import { invoke } from '@tauri-apps/api/core'

export type LaunchTarget = { type: 'jar'; path: string } | { type: 'args_file'; path: string }

export type PublicAccess = 'off' | 'auto'

export type HostedServer = {
	id: string
	name: string
	instance_id: string | null
	game_version: string
	loader: string
	loader_version: string | null
	/** Paper or Purpur, which load plugins instead of mods. */
	platform: PluginPlatform | null
	memory_mb: number
	port: number
	eula_accepted: boolean
	launch: LaunchTarget
	selection: { included: string[]; excluded: string[] } | null
	/** Mods added on the Content tab, kept when updating from the instance. */
	added_content: string[]
	public_access: PublicAccess
	created: string
}

export type PluginPlatform = 'paper' | 'purpur'

export type ServerSoftware =
	| 'vanilla'
	| 'fabric'
	| 'quilt'
	| 'forge'
	| 'neoforge'
	| 'paper'
	| 'purpur'

export type CreateBlankServer = {
	name: string
	software: ServerSoftware
	game_version: string
	seed: string | null
	memory_mb: number | null
	eula_accepted: boolean
}

export type ServerContent = {
	file_name: string
	enabled: boolean
	size: number
	project_id: string | null
	title: string | null
	icon_url: string | null
	version_number: string | null
}

export type ServerInstall = {
	installed: string[]
	missing_dependencies: string[]
	/** Plugins not marked for the server's Minecraft version on Modrinth. */
	unmarked: string[]
}

export type ServerSearchHit = {
	project_id: string
	slug: string
	title: string
	description: string
	author: string
	icon_url: string | null
	downloads: number
}

export type ServerSearchResults = { hits: ServerSearchHit[]; total: number }

/** What the server runs, like "Fabric 1.21.1" or "Paper 1.21.8". */
export function serverSoftwareLabel(server: HostedServer): string {
	const names: Record<string, string> = {
		vanilla: 'Vanilla',
		fabric: 'Fabric',
		quilt: 'Quilt',
		forge: 'Forge',
		neoforge: 'NeoForge',
		paper: 'Paper',
		purpur: 'Purpur',
	}
	const software = server.platform ?? server.loader
	return `${names[software] ?? software} ${server.game_version}`
}

export type WorldSource =
	| { type: 'new'; seed: string | null }
	| { type: 'copy'; instance_id: string; world: string }

export type CreateServer = {
	instance_id: string
	name: string
	included: string[]
	excluded: string[]
	world: WorldSource
	memory_mb: number | null
	eula_accepted: boolean
}

export type EditServer = {
	name?: string
	memory_mb?: number
	port?: number
	eula_accepted?: boolean
	public_access?: PublicAccess
}

export type ServerState = 'offline' | 'starting' | 'running' | 'stopping'

export type PublicAddress = { address: string; via: 'upnp' | 'playit' }

export type ServerStatus = {
	state: ServerState
	started: string | null
	players: string[]
	cpu_percent: number | null
	memory_bytes: number | null
	exit_code: number | null
	public_address: PublicAddress | null
	public_error: string | null
	reachability: Reachability
	lan_address: string | null
}

export type Reachability = 'unchecked' | 'checking' | 'reachable' | 'unreachable'

export type ConsoleLine = {
	seq: number
	stream: 'output' | 'error' | 'input' | 'app'
	text: string
}

export type PlayitLink = {
	linked: boolean
	link_url: string | null
	error: string | null
}

export async function list_servers(): Promise<HostedServer[]> {
	return await invoke('plugin:hosting|hosting_list')
}

export async function get_server(id: string): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_get', { id })
}

export async function create_server(request: CreateServer): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_create', { request })
}

export async function create_blank_server(request: CreateBlankServer): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_create_blank', { request })
}

export async function server_content(id: string): Promise<ServerContent[]> {
	return await invoke('plugin:hosting|hosting_content', { id })
}

/** Returns the file's new name. */
export async function set_server_content_enabled(
	id: string,
	fileName: string,
	enabled: boolean,
): Promise<string> {
	return await invoke('plugin:hosting|hosting_content_set_enabled', { id, fileName, enabled })
}

export async function remove_server_content(id: string, fileName: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_content_remove', { id, fileName })
}

/** Installs a Modrinth project (id or slug) with its required dependencies. */
export async function install_server_project(id: string, project: string): Promise<ServerInstall> {
	return await invoke('plugin:hosting|hosting_install_project', { id, project })
}

export async function search_server_content(
	id: string,
	query: string,
	offset = 0,
): Promise<ServerSearchResults> {
	return await invoke('plugin:hosting|hosting_search_content', { id, query, offset })
}

export async function edit_server(id: string, edit: EditServer): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_edit', { id, edit })
}

export async function delete_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_delete', { id })
}

export async function start_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_start', { id })
}

export async function stop_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_stop', { id })
}

export async function kill_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_kill', { id })
}

export async function send_server_command(id: string, command: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_command', { id, command })
}

export async function server_console(id: string, after: number | null): Promise<ConsoleLine[]> {
	return await invoke('plugin:hosting|hosting_console', { id, after })
}

export async function server_status(id: string): Promise<ServerStatus> {
	return await invoke('plugin:hosting|hosting_status', { id })
}

/** Checks again whether players outside this network can join. */
export async function check_server_reachability(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_check_reachability', { id })
}

export async function server_properties(id: string): Promise<[string, string][]> {
	return await invoke('plugin:hosting|hosting_properties', { id })
}

export async function set_server_properties(
	id: string,
	values: Record<string, string>,
): Promise<void> {
	return await invoke('plugin:hosting|hosting_set_properties', { id, values })
}

export async function sync_server_mods(id: string): Promise<number> {
	return await invoke('plugin:hosting|hosting_sync_mods', { id })
}

export async function server_folder(id: string): Promise<string> {
	return await invoke('plugin:hosting|hosting_folder', { id })
}

export async function playit_link_status(): Promise<PlayitLink> {
	return await invoke('plugin:hosting|playit_link_status')
}

export async function playit_start_link(): Promise<string> {
	return await invoke('plugin:hosting|playit_start_link')
}

export async function playit_unlink(): Promise<void> {
	return await invoke('plugin:hosting|playit_unlink')
}
