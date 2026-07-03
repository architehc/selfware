### FILE: src/prestart.js
<<<<<<< 
const { paths } = require('./constants');
=======
const { paths } = require('./constants');
const wellKnownRouter = require('./src/routes/well-known');

>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
function loadConfig(configFile) {
	nconf.file({
		file: configFile,
	});

	nconf.defaults({
		base_dir: paths.baseDir,
		themes_path: paths.nodeModules,
		upload_path: 'public/uploads',
		views_dir: path.join(paths.baseDir, 'build/public/templates'),
		version: pkg.version,
		isCluster: false,
		isPrimary: true,
		jobsDisabled: false,
		fontawesome: {
			pro: false,
			styles: '*',
		},
	});

	// Explicitly cast as Bool, loader.js passes in isCluster as string 'true'/'false'
	const castAsBool = ['isCluster', 'isPrimary', 'jobsDisabled'];
	nconf.stores.env.readOnly = false;
	castAsBool.forEach((prop) => {
		const value = nconf.get(prop);
		if (value !== undefined) {
			nconf.set(prop, ['1', 1, 'true', true].includes(value));
		}
	});
	nconf.stores.env.readOnly = true;
	nconf.set('runJobs', nconf.get('isPrimary') && !nconf.get('jobsDisabled'));

	// Ensure themes_path is a full filepath
	nconf.set('themes_path', path.resolve(paths.baseDir, nconf.get('themes_path')));
	nconf.set('core_templates_path', path.join(paths.baseDir, 'src/views'));

	nconf.set('upload_path', path.resolve(nconf.get('base_dir'), nconf.get('upload_path')));
	nconf.set('upload_url', '/assets/uploads');

	// nconf defaults, if not set in config
	if (!nconf.get('sessionKey')) {
		nconf.set('sessionKey', 'express.sid');
	}

	if (nconf.get('url')) {
		nconf.set('url', nconf.get('url').replace(/\/$/, ''));
		nconf.set('url_parsed', url.parse(nconf.get('url')));
		// Parse out the relative_url and other goodies from the configured URL
		const urlObject = url.parse(nconf.get('url'));
		const relativePath = urlObject.pathname !== '/' ? urlObject.pathname.replace(/\/+$/, '') : '';
		nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
		nconf.set('secure', urlObject.protocol === 'https:');
		nconf.set('use_port', !!urlObject.port);
		nconf.set('relative_path', relativePath);
		if (!nconf.get('asset_base_url')) {
			nconf.set('asset_base_url', `${relativePath}/assets`);
		}
		nconf.set('port', nconf.get('PORT') || nconf.get('port') || urlObject.port || (nconf.get('PORT_ENV_VAR') ? nconf.get(nconf.get('PORT_ENV_VAR')) : false) || 4567);

		// cookies don't provide isolation by port: http://stackoverflow.com/a/16328399/122353
		const domain = nconf.get('cookieDomain') || urlObject.hostname;
		const origins = nconf.get('socket.io:origins') || `${urlObject.protocol}//${domain}:*`;
		nconf.set('socket.io:origins', origins);
	}
}
>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
function setupWinston() {
	if (!winston.format) {
		return;
	}

	const formats = [];
	if (nconf.get('log-colorize') !== 'false') {
		formats.push(winston.format.colorize());
	}

	if (nconf.get('json-logging')) {
		formats.push(winston.format.timestamp());
		formats.push(winston.format.json());
	} else {
		const timestampFormat = winston.format((info) => {
			const dateString = `${new Date().toISOString()} [${nconf.get('port')}/${global.process.pid}]`;
			info.level = `${dateString} - ${info.level}`;
			return info;
		});
		formats.push(timestampFormat());
		formats.push(winston.format.splat());
		formats.push(winston.format.simple());
	}

	winston.configure({
		level: nconf.get('log-level') || (process.env.NODE_ENV === 'production' ? 'info' : 'verbose'),
		format: winston.format.combine.apply(null, formats),
		transports: [
			new winston.transports.Console({
				handleExceptions: true,
			}),
		],
	});
}
>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
function versionCheck() {
	const version = process.version.slice(1);
	const range = pkg.engines.node;
	const semver = require('semver');
	const compatible = semver.satisfies(version, range);

	if (!compatible) {
		winston.warn('Your version of Node.js is too outdated for NodeBB. Please update your version of Node.js.');
		winston.warn(`Recommended ${chalk.green(range)}, ${chalk.yellow(version)} provided\n`);
	}
}
>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
exports.setupWinston = setupWinston;
exports.loadConfig = loadConfig;
exports.versionCheck = versionCheck;
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
const db = require('../../src/database');
const meta = require('../../src/meta');
const webserver = require('../../src/webserver');
const sockets = require('../../src/socket.io');
const notifications = require('../../src/notifications');
const user = require('../../src/user');
const privileges = require('../../src/privileges');
const groups = require('../../src/groups');
const posts = require('../../src/posts/cache');
const cache = require('../../src/cache');
const uploads = require('../../src/middleware/uploads');
const { mkdirp } = require('mkdirp');

const fs = require('fs');
const path = require('path');

const url = require('url');
const nconf = require('nconf');
const winston = require('winston');
const packageInfo = require('../../package.json');

winston.add(new winston.transports.Console({
	format: winston.format.combine(
		winston.format.splat(),
		winston.format.simple()
	),
}));

try {
	const configJSON = fs.readFileSync(path.join(__dirname, '../../config.json'), 'utf-8');
	winston.info('configJSON');
	winston.info(configJSON);
} catch (err) {
	console.error(err.stack);
	throw err;
}

nconf.file({ file: path.join(__dirname, '../../config.json') });
nconf.defaults({
	base_dir: path.join(__dirname, '../..'),
	themes_path: path.join(__dirname, '../../node_modules'),
	upload_path: 'test/uploads',
	views_dir: path.join(__dirname, '../../build/public/templates'),
	relative_path: '',
});

const urlObject = url.parse(nconf.get('url'));
const relativePath = urlObject.pathname !== '/' ? urlObject.pathname : '';
nconf.set('relative_path', relativePath);
nconf.set('asset_base_url', `${relativePath}/assets`);
nconf.set('upload_path', path.join(nconf.get('base_dir'), nconf.get('upload_path')));
nconf.set('upload_url', '/assets/uploads');
nconf.set('url_parsed', urlObject);
nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
nconf.set('secure', urlObject.protocol === 'https:');
nconf.set('use_port', !!urlObject.port);
nconf.set('port', urlObject.port || nconf.get('port') || (nconf.get('PORT_ENV_VAR') ? nconf.get(nconf.get('PORT_ENV_VAR')) : false) || 4567);

// cookies don't provide isolation by port: http://stackoverflow.com/a/16328399/122353
const domain = nconf.get('cookieDomain') || urlObject.hostname;
const origins = nconf.get('socket.io:origins') || `${urlObject.protocol}//${domain}:*`;
nconf.set('socket.io:origins', origins);

if (nconf.get('isCluster') === undefined) {
	nconf.set('isPrimary', true);
	nconf.set('isCluster', false);
	nconf.set('singleHostCluster', false);
}

const dbType = nconf.get('database');
const testDbConfig = nconf.get('test_database');
const productionDbConfig = nconf.get(dbType);

if (!testDbConfig) {
	const errorText = 'test_database is not defined';
	winston.info(
		'\n===========================================================\n' +
		'Please, add parameters for test database in config.json\n' +
		'For example (redis):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "6379",\n' +
		'    "password": "",\n' +
		'    "database": "1"\n' +
		'}\n' +
		' or (mongo):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "27017",\n' +
		'    "password": "",\n' +
		'    "database": "1"\n' +
		'}\n' +
		' or (mongo) in a replicaset\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1,127.0.0.1,127.0.0.1",\n' +
		'    "port": "27017,27018,27019",\n' +
		'    "username": "",\n' +
		'    "password": "",\n' +
		'    "database": "nodebb_test"\n' +
		'}\n' +
		' or (postgres):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "5432",\n' +
		'    "username": "postgres",\n' +
		'    "password": "",\n' +
		'    "database": "nodebb_test"\n' +
		'}\n' +
		'==========================================================='
	);
	winston.error(errorText);
	throw new Error(errorText);
}

if (testDbConfig.database === productionDbConfig.database &&
	testDbConfig.host === productionDbConfig.host &&
	testDbConfig.port === productionDbConfig.port) {
	const errorText = 'test_database has the same config as production db';
	winston.error(errorText);
	throw new Error(errorText);
}

nconf.set(dbType, testDbConfig);

winston.info('database config %s', dbType, testDbConfig);
winston.info(`environment ${global.env}`);

const db = require('../../src/database');

module.exports = db;

before(async function () {
	this.timeout(30000);

	// Parse out the relative_url and other goodies from the configured URL
	const urlObject = url.parse(nconf.get('url'));

	nconf.set('core_templates_path', path.join(__dirname, '../../src/views'));
	nconf.set('base_templates_path', path.join(nconf.get('themes_path'), 'nodebb-theme-persona/templates'));
	nconf.set('theme_config', path.join(nconf.get('themes_path'), 'nodebb-theme-persona', 'theme.json'));
	nconf.set('bcrypt_rounds', 1);
	nconf.set('socket.io:origins', '*:*');
	nconf.set('version', packageInfo.version);
	nconf.set('runJobs', false);
	nconf.set('jobsDisabled', false);

	await db.init();
	if (db.hasOwnProperty('createIndices')) {
		await db.createIndices();
	}
	await setupMockDefaults();
	await db.initSessionStore();

	const meta = require('../../src/meta');
	nconf.set('theme_templates_path', meta.config['theme:templates'] ? path.join(nconf.get('themes_path'), meta.config['theme:id'], meta.config['theme:templates']) : nconf.get('base_templates_path'));
	// nconf defaults, if not set in config
	if (!nconf.get('sessionKey')) {
		nconf.set('sessionKey', 'express.sid');
	}

	await meta.dependencies.check();

	const webserver = require('../../src/webserver');
	const sockets = require('../../src/socket.io');
	await sockets.init(webserver.server);

	require('../../src/notifications').startJobs();
	require('../../src/user').startJobs();

	await webserver.listen();

	// Iterate over all of the test suites/contexts
	this.test.parent.suites.forEach((suite) => {
		// Attach an afterAll listener that resets the defaults
		suite.afterAll(async () => {
			await setupMockDefaults();
		});
	});
});

async function setupMockDefaults() {
	const meta = require('../../src/meta');
	await db.emptydb();

	winston.info('test_database flushed');
	await setupDefaultConfigs(meta);

	await meta.configs.init();
	meta.config.postDelay = 0;
	meta.config.initialPostDelay = 0;
	meta.config.newbiePostDelay = 0;
	meta.config.autoDetectLang = 0;

	require('../../src/groups').cache.reset();
	require('../../src/posts/cache').reset();
	require('../../src/cache').reset();
	require('../../src/middleware/uploads').clearCache();
	// privileges must be given after cache reset
	await giveDefaultGlobalPrivileges();
	await enableDefaultPlugins();

	await meta.themes.set({
		type: 'local',
		id: 'nodebb-theme-persona',
	});

	const fs = require('fs');
	await fs.promises.rm('test/uploads', { recursive: true, force: true });

	const { mkdirp } = require('mkdirp');

	const folders = [
		'test/uploads',
		'test/uploads/category',
		'test/uploads/files',
		'test/uploads/system',
		'test/uploads/profile',
	];
	for (const folder of folders) {
		/* eslint-disable no-await-in-loop */
		await mkdirp(folder);
	}
}
db.setupMockDefaults = setupMockDefaults;

async function setupDefaultConfigs(meta) {
	winston.info('Populating database with default configs, if not already set...\n');

	const defaults = require(path.join(nconf.get('base_dir'), 'install/data/defaults.json'));
	defaults.eventLoopCheckEnabled = 0;
	defaults.minimumPasswordStrength = 0;
	await meta.configs.setOnEmpty(defaults);
}

async function giveDefaultGlobalPrivileges() {
	winston.info('Giving default global privileges...\n');
	const privileges = require('../../src/privileges');
	await privileges.global.give([
		'groups:chat', 'groups:upload:post:image', 'groups:signature', 'groups:search:content',
		'groups:search:users', 'groups:search:tags', 'groups:local:login', 'groups:view:users',
		'groups:view:tags', 'groups:view:groups',
	], 'registered-users');
	await privileges.global.give([
		'groups:view:users', 'groups:view:tags', 'groups:view:groups',
	], 'guests');
}

async function enableDefaultPlugins() {
	winston.info('Enabling default plugins\n');
	const testPlugins = Array.isArray(nconf.get('test_plugins')) ? nconf.get('test_plugins') : [];
	const defaultEnabled = [
		'nodebb-plugin-dbsearch',
		'nodebb-widget-essentials',
		'nodebb-plugin-composer-default',
	].concat(testPlugins);

	winston.info('[install/enableDefaultPlugins] activating default plugins', defaultEnabled);

	await db.sortedSetAdd('plugins:active', Object.keys(defaultEnabled), defaultEnabled);
}
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
const db = require('../../src/database');
const meta = require('../../src/meta');
const webserver = require('../../src/webserver');
const sockets = require('../../src/socket.io');
const notifications = require('../../src/notifications');
const user = require('../../src/user');
const privileges = require('../../src/privileges');
const groups = require('../../src/groups');
const posts = require('../../src/posts/cache');
const cache = require('../../src/cache');
const uploads = require('../../src/middleware/uploads');
const { mkdirp } = require('mkdirp');

const fs = require('fs');
const path = require('path');

const url = require('url');
const nconf = require('nconf');
const winston = require('winston');
const packageInfo = require('../../package.json');

winston.add(new winston.transports.Console({
	format: winston.format.combine(
		winston.format.splat(),
		winston.format.simple()
	),
}));

try {
	const configJSON = fs.readFileSync(path.join(__dirname, '../../config.json'), 'utf-8');
	winston.info('configJSON');
	winston.info(configJSON);
} catch (err) {
	console.error(err.stack);
	throw err;
}

nconf.file({ file: path.join(__dirname, '../../config.json') });
nconf.defaults({
	base_dir: path.join(__dirname, '../..'),
	themes_path: path.join(__dirname, '../../node_modules'),
	upload_path: 'test/uploads',
	views_dir: path.join(__dirname, '../../build/public/templates'),
	relative_path: '',
});

const urlObject = url.parse(nconf.get('url'));
const relativePath = urlObject.pathname !== '/' ? urlObject.pathname : '';
nconf.set('relative_path', relativePath);
nconf.set('asset_base_url', `${relativePath}/assets`);
nconf.set('upload_path', path.join(nconf.get('base_dir'), nconf.get('upload_path')));
nconf.set('upload_url', '/assets/uploads');
nconf.set('url_parsed', urlObject);
nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
nconf.set('secure', urlObject.protocol === 'https:');
nconf.set('use_port', !!urlObject.port);
nconf.set('port', urlObject.port || nconf.get('port') || (nconf.get('PORT_ENV_VAR') ? nconf.get(nconf.get('PORT_ENV_VAR')) : false) || 4567);

// cookies don't provide isolation by port: http://stackoverflow.com/a/16328399/122353
const domain = nconf.get('cookieDomain') || urlObject.hostname;
const origins = nconf.get('socket.io:origins') || `${urlObject.protocol}//${domain}:*`;
nconf.set('socket.io:origins', origins);

if (nconf.get('isCluster') === undefined) {
	nconf.set('isPrimary', true);
	nconf.set('isCluster', false);
	nconf.set('singleHostCluster', false);
}

const dbType = nconf.get('database');
const testDbConfig = nconf.get('test_database');
const productionDbConfig = nconf.get(dbType);

if (!testDbConfig) {
	const errorText = 'test_database is not defined';
	winston.info(
		'\n===========================================================\n' +
		'Please, add parameters for test database in config.json\n' +
		'For example (redis):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "6379",\n' +
		'    "password": "",\n' +
		'    "database": "1"\n' +
		'}\n' +
		' or (mongo):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "27017",\n' +
		'    "password": "",\n' +
		'    "database": "1"\n' +
		'}\n' +
		' or (mongo) in a replicaset\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1,127.0.0.1,127.0.0.1",\n' +
		'    "port": "27017,27018,27019",\n' +
		'    "username": "",\n' +
		'    "password": "",\n' +
		'    "database": "nodebb_test"\n' +
		'}\n' +
		' or (postgres):\n' +
		'"test_database": {\n' +
		'    "host": "127.0.0.1",\n' +
		'    "port": "5432",\n' +
		'    "username": "postgres",\n' +
		'    "password": "",\n' +
		'    "database": "nodebb_test"\n' +
		'}\n' +
		'==========================================================='
	);
	winston.error(errorText);
	throw new Error(errorText);
}

if (testDbConfig.database === productionDbConfig.database &&
	testDbConfig.host === productionDbConfig.host &&
	testDbConfig.port === productionDbConfig.port) {
	const errorText = 'test_database has the same config as production db';
	winston.error(errorText);
	throw new Error(errorText);
}

nconf.set(dbType, testDbConfig);

winston.info('database config %s', dbType, testDbConfig);
winston.info(`environment ${global.env}`);

const db = require('../../src/database');

module.exports = db;

before(async function () {
	this.timeout(30000);

	// Parse out the relative_url and other goodies from the configured URL
	const urlObject = url.parse(nconf.get('url'));

	nconf.set('core_templates_path', path.join(__dirname, '../../src/views'));
	nconf.set('base_templates_path', path.join(nconf.get('themes_path'), 'nodebb-theme-persona/templates'));
	nconf.set('theme_config', path.join(nconf.get('themes_path'), 'nodebb-theme-persona', 'theme.json'));
	nconf.set('bcrypt_rounds', 1);
	nconf.set('socket.io:origins', '*:*');
	nconf.set('version', packageInfo.version);
	nconf.set('runJobs', false);
	nconf.set('jobsDisabled', false);

	await db.init();
	if (db.hasOwnProperty('createIndices')) {
		await db.createIndices();
	}
	await setupMockDefaults();
	await db.initSessionStore();

	const meta = require('../../src/meta');
	nconf.set('theme_templates_path', meta.config['theme:templates'] ? path.join(nconf.get('themes_path'), meta.config['theme:id'], meta.config['theme:templates']) : nconf.get('base_templates_path'));
	// nconf defaults, if not set in config
	if (!nconf.get('sessionKey')) {
		nconf.set('sessionKey', 'express.sid');
	}

	await meta.dependencies.check();

	const webserver = require('../../src/webserver');
	const sockets = require('../../src/socket.io');
	await sockets.init(webserver.server);

	require('../../src/notifications').startJobs();
	require('../../src/user').startJobs();

	await webserver.listen();

	// Iterate over all of the test suites/contexts
	this.test.parent.suites.forEach((suite) => {
		// Attach an afterAll listener that resets the defaults
		suite.afterAll(async () => {
			await setupMockDefaults();
		});
	});
});

async function setupMockDefaults() {
	const meta = require('../../src/meta');
	await db.emptydb();

	winston.info('test_database flushed');
	await setupDefaultConfigs(meta);

	await meta.configs.init();
	meta.config.postDelay = 0;
	meta.config.initialPostDelay = 0;
	meta.config.newbiePostDelay = 0;
	meta.config.autoDetectLang = 0;

	require('../../src/groups').cache.reset();
	require('../../src/posts/cache').reset();
	require('../../src/cache').reset();
	require('../../src/middleware/uploads').clearCache();
	// privileges must be given after cache reset
	await giveDefaultGlobalPrivileges();
	await enableDefaultPlugins();

	await meta.themes.set({
		type: 'local',
		id: 'nodebb-theme-persona',
	});

	const fs = require('fs');
	await fs.promises.rm('test/uploads', { recursive: true, force: true });

	const { mkdirp } = require('mkdirp');

	const folders = [
		'test/uploads',
		'test/uploads/category',
		'test/uploads/files',
		'test/uploads/system',
		'test/uploads/profile',
	];
	for (const folder of folders) {
		/* eslint-disable no-await-in-loop */
		await mkdirp(folder);
	}
}
db.setupMockDefaults = setupMockDefaults;

async function setupDefaultConfigs(meta) {
	winston.info('Populating database with default configs, if not already set...\n');

	const defaults = require(path.join(nconf.get('base_dir'), 'install/data/defaults.json'));
	defaults.eventLoopCheckEnabled = 0;
	defaults.minimumPasswordStrength = 0;
	await meta.configs.setOnEmpty(defaults);
}

async function giveDefaultGlobalPrivileges() {
	winston.info('Giving default global privileges...\n');
	const privileges = require('../../src/privileges');
	await privileges.global.give([
		'groups:chat', 'groups:upload:post:image', 'groups:signature', 'groups:search:content',
		'groups:search:users', 'groups:search:tags', 'groups:local:login', 'groups:view:users',
		'groups:view:tags', 'groups:view:groups',
	], 'registered-users');
	await privileges.global.give([
		'groups:view:users', 'groups:view:tags', 'groups:view:groups',
	], 'guests');
}

async function enableDefaultPlugins() {
	winston.info('Enabling default plugins\n');
	const testPlugins = Array.isArray(nconf.get('test_plugins')) ? nconf.get('test_plugins') : [];
	const defaultEnabled = [
		'nodebb-plugin-dbsearch',
		'nodebb-widget-essentials',
		'nodebb-plugin-composer-default',
	].concat(testPlugins);

	winston.info('[install/enableDefaultPlugins] activating default plugins', defaultEnabled);

	await db.sortedSetAdd('plugins:active', Object.keys(defaultEnabled), defaultEnabled);
}
>>>>>>> 

### FILE: webpack.installer.js
<<<<<<< 
module.exports = {
	mode: 'production',
	entry: {
		installer: './public/src/installer/install.js',
	},
	output: {
		filename: '[name].min.js',
		path: path.resolve(__dirname, 'build/public'),
		publicPath: `/assets/`,
	},
	resolve: {
		symlinks: false,
		modules: [
			'public/src',
			'node_modules',
		],
	},
};
>>>>>>> 

### FILE: webpack.installer.js
<<<<<<< 
module.exports = {
	mode: 'production',
	entry: {
		installer: './public/src/installer/install.js',
	},
	output: {
		filename: '[name].min.js',
		path: path.resolve(__dirname, 'build/public'),
		publicPath: `/assets/`,
	},
	resolve: {
		symlinks: false,
		modules: [
			'public/src',
			'node_modules',
		],
	},
};
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const file = require('./src/file');
const pkg = require('./package.json');

const pathToConfig = path.resolve(__dirname, process.env.CONFIG || 'config.json');

nconf.argv().env().file({
	file: pathToConfig,
});

const pidFilePath = path.join(__dirname, 'pidfile');

const outputLogFilePath = path.join(__dirname, nconf.get('logFile') || 'logs/output.log');

const logDir = path.dirname(outputLogFilePath);
if (!fs.existsSync(logDir)) {
	mkdirp.sync(path.dirname(outputLogFilePath));
}

const output = logrotate({ file: outputLogFilePath, size: '1m', keep: 3, compress: true });
const silent = nconf.get('silent') === 'false' ? false : nconf.get('silent') !== false;
let numProcs;
const workers = [];
const Loader = {};
const appPath = path.join(__dirname, 'app.js');

Loader.init = function () {
	if (silent) {
		console.log = (...args) => {
			output.write(`${args.join(' ')}\n`);
		};
	}

	process.on('SIGHUP', Loader.restart);
	process.on('SIGTERM', Loader.stop);
};

Loader.displayStartupMessages = function () {
	console.log('');
	console.log(`NodeBB v${pkg.version} Copyright (C) 2013-${(new Date()).getFullYear()} NodeBB Inc.`);
	console.log('This program comes with ABSOLUTELY NO WARRANTY.');
	console.log('This is free software, and you are welcome to redistribute it under certain conditions.');
	console.log('For the full license, please visit: http://www.gnu.org/copyleft/gpl.html');
	console.log('');
};

Loader.addWorkerEvents = function (worker) {
	worker.on('exit', (code, signal) => {
		console.log(`[cluster] Child Process (${worker.pid}) has exited (code: ${code}, signal: ${signal})`);
		if (!(worker.suicide || code === 0)) {
			console.log('[cluster] Spinning up another process...');

			forkWorker(worker.index, worker.isPrimary);
		}
	});

	worker.on('message', (message) => {
		if (message && typeof message === 'object' && message.action) {
			switch (message.action) {
				case 'restart':
					console.log('[cluster] Restarting...');
					Loader.restart();
					break;
				case 'pubsub':
					workers.forEach((w) => {
						w.send(message);
					});
					break;
				case 'socket.io':
					workers.forEach((w) => {
						if (w !== worker) {
							w.send(message);
						}
					});
					break;
			}
		}
	});
};

Loader.start = function () {
	numProcs = getPorts().length;
	console.log(`Clustering enabled: Spinning up ${numProcs} process(es).\n`);

	for (let x = 0; x < numProcs; x += 1) {
		forkWorker(x, x === 0);
	}
};

function forkWorker(index, isPrimary) {
	const ports = getPorts();
	const args = [];

	if (!ports[index]) {
		return console.log(`[cluster] invalid port for worker : ${index} ports: ${ports.length}`);
	}

	process.env.isPrimary = isPrimary;
	process.env.isCluster = nconf.get('isCluster') || ports.length > 1;
	process.env.port = ports[index];

	const worker = fork(appPath, args, {
		silent: silent,
		env: process.env,
	});

	worker.index = index;
	worker.isPrimary = isPrimary;

	workers[index] = worker;

	Loader.addWorkerEvents(worker);

	if (silent) {
		const output = logrotate({ file: outputLogFilePath, size: '1m', keep: 3, compress: true });
		worker.stdout.pipe(output);
		worker.stderr.pipe(output);
	}
}

function getPorts() {
	const _url = nconf.get('url');
	if (!_url) {
		console.log('[cluster] url is undefined, please check your config.json');
		process.exit();
	}
	const urlObject = url.parse(_url);
	let port = nconf.get('PORT') || nconf.get('port') || urlObject.port || 4567;
	if (!Array.isArray(port)) {
		port = [port];
	}
	return port;
}

Loader.restart = function () {
	killWorkers();

	nconf.remove('file');
	nconf.use('file', { file: pathToConfig });

	fs.readFile(pathToConfig, { encoding: 'utf-8' }, (err, configFile) => {
		if (err) {
			console.error('Error reading config');
			throw err;
		}

		const conf = JSON.parse(configFile);

		nconf.stores.env.readOnly = false;
		nconf.set('url', conf.url);
		nconf.stores.env.readOnly = true;

		if (process.env.url !== conf.url) {
			process.env.url = conf.url;
		}
		Loader.start();
	});
};

Loader.stop = function () {
	killWorkers();

	// Clean up the pidfile
	if (nconf.get('daemon') !== 'false' && nconf.get('daemon') !== false) {
		fs.unlinkSync(pidFilePath);
	}
};

function killWorkers() {
	workers.forEach((worker) => {
		worker.suicide = true;
		worker.kill();
	});
}

fs.open(pathToConfig, 'r', (err) => {
	if (err) {
		// No config detected, kickstart web installer
		fork('app');
		return;
	}

	if (nconf.get('daemon') !== 'false' && nconf.get('daemon') !== false) {
		if (file.existsSync(pidFilePath)) {
			let pid = 0;
			try {
				pid = fs.readFileSync(pidFilePath, { encoding: 'utf-8' });
				if (pid) {
					process.kill(pid, 0);
					console.info(`Process "${pid}" from pidfile already running, exiting`);
					process.exit();
				} else {
					console.info(`Invalid pid "${pid}" from pidfile, deleting pidfile`);
					fs.unlinkSync(pidFilePath);
				}
			} catch (err) {
				if (err.code === 'ESRCH') {
					console.info(`Process "${pid}" from pidfile not found, deleting pidfile`);
					fs.unlinkSync(pidFilePath);
				} else {
					console.error(err.stack);
					throw err;
				}
			}
		}

		require('daemon')({
			stdout: process.stdout,
			stderr: process.stderr,
			cwd: process.cwd(),
		});

		fs.writeFileSync(pidFilePath, String(process.pid));
	}
	try {
		Loader.init();
		Loader.displayStartupMessages();
		Loader.start();
	} catch (err) {
		console.error('[loader] Error during startup');
		throw err;
	}
});
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const file = require('./src/file');
const pkg = require('./package.json');

const pathToConfig = path.resolve(__dirname, process.env.CONFIG || 'config.json');

nconf.argv().env().file({
	file: pathToConfig,
});

const pidFilePath = path.join(__dirname, 'pidfile');

const outputLogFilePath = path.join(__dirname, nconf.get('logFile') || 'logs/output.log');

const logDir = path.dirname(outputLogFilePath);
if (!fs.existsSync(logDir)) {
	mkdirp.sync(path.dirname(outputLogFilePath));
}

const output = logrotate({ file: outputLogFilePath, size: '1m', keep: 3, compress: true });
const silent = nconf.get('silent') === 'false' ? false : nconf.get('silent') !== false;
let numProcs;
const workers = [];
const Loader = {};
const appPath = path.join(__dirname, 'app.js');

Loader.init = function () {
	if (silent) {
		console.log = (...args) => {
			output.write(`${args.join(' ')}\n`);
		};
	}

	process.on('SIGHUP', Loader.restart);
	process.on('SIGTERM', Loader.stop);
};

Loader.displayStartupMessages = function () {
	console.log('');
	console.log(`NodeBB v${pkg.version} Copyright (C) 2013-${(new Date()).getFullYear()} NodeBB Inc.`);
	console.log('This program comes with ABSOLUTELY NO WARRANTY.');
	console.log('This is free software, and you are welcome to redistribute it under certain conditions.');
	console.log('For the full license, please visit: http://www.gnu.org/copyleft/gpl.html');
	console.log('');
};

Loader.addWorkerEvents = function (worker) {
	worker.on('exit', (code, signal) => {
		console.log(`[cluster] Child Process (${worker.pid}) has exited (code: ${code}, signal: ${signal})`);
		if (!(worker.suicide || code === 0)) {
			console.log('[cluster] Spinning up another process...');

			forkWorker(worker.index, worker.isPrimary);
		}
	});

	worker.on('message', (message) => {
		if (message && typeof message === 'object' && message.action) {
			switch (message.action) {
				case 'restart':
					console.log('[cluster] Restarting...');
					Loader.restart();
					break;
				case 'pubsub':
					workers.forEach((w) => {
						w.send(message);
					});
					break;
				case 'socket.io':
					workers.forEach((w) => {
						if (w !== worker) {
							w.send(message);
						}
					});
					break;
			}
		}
	});
};

Loader.start = function () {
	numProcs = getPorts().length;
	console.log(`Clustering enabled: Spinning up ${numProcs} process(es).\n`);

	for (let x = 0; x < numProcs; x += 1) {
		forkWorker(x, x === 0);
	}
};

function forkWorker(index, isPrimary) {
	const ports = getPorts();
	const args = [];

	if (!ports[index]) {
		return console.log(`[cluster] invalid port for worker : ${index} ports: ${ports.length}`);
	}

	process.env.isPrimary = isPrimary;
	process.env.isCluster = nconf.get('isCluster') || ports.length > 1;
	process.env.port = ports[index];

	const worker = fork(appPath, args, {
		silent: silent,
		env: process.env,
	});

	worker.index = index;
	worker.isPrimary = isPrimary;

	workers[index] = worker;

	Loader.addWorkerEvents(worker);

	if (silent) {
		const output = logrotate({ file: outputLogFilePath, size: '1m', keep: 3, compress: true });
		worker.stdout.pipe(output);
		worker.stderr.pipe(output);
	}
}

function getPorts() {
	const _url = nconf.get('url');
	if (!_url) {
		console.log('[cluster] url is undefined, please check your config.json');
		process.exit();
	}
	const urlObject = url.parse(_url);
	let port = nconf.get('PORT') || nconf.get('port') || urlObject.port || 4567;
	if (!Array.isArray(port)) {
		port = [port];
	}
	return port;
}

Loader.restart = function () {
	killWorkers();

	nconf.remove('file');
	nconf.use('file', { file: pathToConfig });

	fs.readFile(pathToConfig, { encoding: 'utf-8' }, (err, configFile) => {
		if (err) {
			console.error('Error reading config');
			throw err;
		}

		const conf = JSON.parse(configFile);

		nconf.stores.env.readOnly = false;
		nconf.set('url', conf.url);
		nconf.stores.env.readOnly = true;

		if (process.env.url !== conf.url) {
			process.env.url = conf.url;
		}
		Loader.start();
	});
};

Loader.stop = function () {
	killWorkers();

	// Clean up the pidfile
	if (nconf.get('daemon') !== 'false' && nconf.get('daemon') !== false) {
		fs.unlinkSync(pidFilePath);
	}
};

function killWorkers() {
	workers.forEach((worker) => {
		worker.suicide = true;
		worker.kill();
	});
}

fs.open(pathToConfig, 'r', (err) => {
	if (err) {
		// No config detected, kickstart web installer
		fork('app');
		return;
	}

	if (nconf.get('daemon') !== 'false' && nconf.get('daemon') !== false) {
		if (file.existsSync(pidFilePath)) {
			let pid = 0;
			try {
				pid = fs.readFileSync(pidFilePath, { encoding: 'utf-8' });
				if (pid) {
					process.kill(pid, 0);
					console.info(`Process "${pid}" from pidfile already running, exiting`);
					process.exit();
				} else {
					console.info(`Invalid pid "${pid}" from pidfile, deleting pidfile`);
					fs.unlinkSync(pidFilePath);
				}
			} catch (err) {
				if (err.code === 'ESRCH') {
					console.info(`Process "${pid}" from pidfile not found, deleting pidfile`);
					fs.unlinkSync(pidFilePath);
				} else {
					console.error(err.stack);
					throw err;
				}
			}
		}

		require('daemon')({
			stdout: process.stdout,
			stderr: process.stderr,
			cwd: process.cwd(),
		});

		fs.writeFileSync(pidFilePath, String(process.pid));
	}
	try {
		Loader.init();
		Loader.displayStartupMessages();
		Loader.start();
	} catch (err) {
		console.error('[loader] Error during startup');
		throw err;
	}
});
>>>>>>> 

### FILE: Gruntfile.js
<<<<<<< 
const path = require('path');
const nconf = require('nconf');

nconf.argv().env({
	separator: '__',
});
const winston = require('winston');
const { fork } = require('child_process');

const { env } = process;
let worker;

env.NODE_ENV = env.NODE_ENV || 'development';

const configFile = path.resolve(__dirname, nconf.any(['config', 'CONFIG']) || 'config.json');
const prestart = require('./src/prestart');

prestart.loadConfig(configFile);

const db = require('./src/database');
const plugins = require('./src/plugins');

module.exports = function (grunt) {
	const args = [];

	if (!grunt.option('verbose')) {
		args.push('--log-level=info');
		nconf.set('log-level', 'info');
	}
	prestart.setupWinston();

	grunt.initConfig({
		watch: {},
	});

	grunt.loadNpmTasks('grunt-contrib-watch');

	grunt.registerTask('default', ['watch']);

	grunt.registerTask('init', async function () {
		const done = this.async();
		let pluginList = [];
		if (!process.argv.includes('--core')) {
			await db.init();
			pluginList = await plugins.getActive();
			addBaseThemes(pluginList);
			if (!pluginList.includes('nodebb-plugin-composer-default')) {
				pluginList.push('nodebb-plugin-composer-default');
			}
			if (!pluginList.includes('nodebb-theme-harmony')) {
				pluginList.push('nodebb-theme-harmony');
			}
		}

		const styleUpdated_Client = pluginList.map(p => `node_modules/${p}/*.scss`)
			.concat(pluginList.map(p => `node_modules/${p}/*.css`))
			.concat(pluginList.map(p => `node_modules/${p}/+(public|static|scss)/**/*.scss`))
			.concat(pluginList.map(p => `node_modules/${p}/+(public|static)/**/*.css`));

		const clientUpdated = pluginList.map(p => `node_modules/${p}/+(public|static)/**/*.js`);
		const serverUpdated = pluginList.map(p => `node_modules/${p}/*.js`)
			.concat(pluginList.map(p => `node_modules/${p}/+(lib|src)/**/*.js`));

		const templatesUpdated = pluginList.map(p => `node_modules/${p}/+(public|static|templates)/**/*.tpl`);
		const langUpdated = pluginList.map(p => `node_modules/${p}/+(public|static|languages)/**/*.json`);
		const interval = 100;
		grunt.config(['watch'], {
			styleUpdated: {
				files: [
					'public/scss/**/*.scss',
					...styleUpdated_Client,
				],
				options: {
					interval: interval,
				},
			},
			clientUpdated: {
				files: [
					'public/src/**/*.js',
					'public/vendor/**/*.js',
					...clientUpdated,
					'node_modules/benchpressjs/build/benchpress.js',
				],
				options: {
					interval: interval,
				},
			},
			serverUpdated: {
				files: [
					'app.js',
					'install/*.js',
					'src/**/*.js',
					'public/src/modules/translator.common.js',
					'public/src/modules/helpers.common.js',
					'public/src/utils.common.js',
					serverUpdated,
					'!src/upgrades/**',
				],
				options: {
					interval: interval,
				},
			},
			templatesUpdated: {
				files: [
					'src/views/**/*.tpl',
					...templatesUpdated,
				],
				options: {
					interval: interval,
				},
			},
			langUpdated: {
				files: [
					'public/language/en-GB/*.json',
					'public/language/en-GB/**/*.json',
					...langUpdated,
				],
				options: {
					interval: interval,
				},
			},
		});
		const build = require('./src/meta/build');
		if (!grunt.option('skip')) {
			await build.build(true, { watch: true });
		}
		run();
		done();
	});

	function run() {
		if (worker) {
			worker.kill();
		}

		const execArgv = [];
		const inspect = process.argv.find(a => a.startsWith('--inspect'));

		if (inspect) {
			execArgv.push(inspect);
		}

		worker = fork('app.js', args, {
			env,
			execArgv,
		});
	}

	grunt.task.run('init');

	grunt.event.removeAllListeners('watch');
	grunt.event.on('watch', (action, filepath, target) => {
		let compiling;
		if (target === 'styleUpdated') {
			compiling = ['clientCSS', 'acpCSS'];
		} else if (target === 'clientUpdated') {
			compiling = ['js'];
		} else if (target === 'templatesUpdated') {
			compiling = ['tpl'];
		} else if (target === 'langUpdated') {
			compiling = ['lang'];
		} else if (target === 'serverUpdated') {
			// empty require cache
			const paths = ['./src/meta/build.js', './src/meta/index.js'];
			paths.forEach(p => delete require.cache[require.resolve(p)]);
			return run();
		}

		require('./src/meta/build').build(compiling, { webpack: false }, (err) => {
			if (err) {
				winston.error(err.stack);
			}
			if (worker) {
				worker.send({ compiling: compiling });
			}
		});
	});
};

function addBaseThemes(pluginList) {
	let themeId = pluginList.find(p => p.includes('nodebb-theme-'));
	if (!themeId) {
		return pluginList;
	}
	let baseTheme;
	do {
		try {
			baseTheme = require(`${themeId}/theme`).baseTheme;
		} catch (err) {
			console.log(err);
		}

		if (baseTheme) {
			pluginList.push(baseTheme);
			themeId = baseTheme;
		}
	} while (baseTheme);
	return pluginList;
}
>>>>>>> 

### FILE: Gruntfile.js
<<<<<<< 
const path = require('path');
const nconf = require('nconf');

nconf.argv().env({
	separator: '__',
});
const winston = require('winston');
const { fork } = require('child_process');

const { env } = process;
let worker;

env.NODE_ENV = env.NODE_ENV || 'development';

const configFile = path.resolve(__dirname, nconf.any(['config', 'CONFIG']) || 'config.json');
const prestart = require('./src/prestart');

prestart.loadConfig(configFile);

const db = require('./src/database');
const plugins = require('./src/plugins');

module.exports = function (grunt) {
	const args = [];

	if (!grunt.option('verbose')) {
		args.push('--log-level=info');
		nconf.set('log-level', 'info');
	}
	prestart.setupWinston();

	grunt.initConfig({
		watch: {},
	});

	grunt.loadNpmTasks('grunt-contrib-watch');

	grunt.registerTask('default', ['watch']);

	grunt.registerTask('init', async function () {
		const done = this.async();
		let pluginList = [];
		if (!process.argv.includes('--core')) {
			await db.init();
			pluginList = await plugins.getActive();
			addBaseThemes(pluginList);
			if (!pluginList.includes('nodebb-plugin-composer-default')) {
				pluginList.push('nodebb-plugin-composer-default');
			}
			if (!pluginList.includes('nodebb-theme-harmony')) {
				pluginList.push('nodebb-theme-harmony');
			}
		}

		const styleUpdated_Client = pluginList.map(p => `node_modules/${p}/*.scss`)
			.concat(pluginList.map(p => `node_modules/${p}/*.css`))
			.concat(pluginList.map(p => `node_modules/${p}/+(public|static|scss)/**/*.scss`))
			.concat(pluginList.map(p => `node_modules/${p}/+(public|static)/**/*.css`));

		const clientUpdated = pluginList.map(p => `node_modules/${p}/+(public|static)/**/*.js`);
		const serverUpdated = pluginList.map(p => `node_modules/${p}/*.js`)
			.concat(pluginList.map(p => `node_modules/${p}/+(lib|src)/**/*.js`));

		const templatesUpdated = pluginList.map(p => `node_modules/${p}/+(public|static|templates)/**/*.tpl`);
		const langUpdated = pluginList.map(p => `node_modules/${p}/+(public|static|languages)/**/*.json`);
		const interval = 100;
		grunt.config(['watch'], {
			styleUpdated: {
				files: [
					'public/scss/**/*.scss',
					...styleUpdated_Client,
				],
				options: {
					interval: interval,
				},
			},
			clientUpdated: {
				files: [
					'public/src/**/*.js',
					'public/vendor/**/*.js',
					...clientUpdated,
					'node_modules/benchpressjs/build/benchpress.js',
				],
				options: {
					interval: interval,
				},
			},
			serverUpdated: {
				files: [
					'app.js',
					'install/*.js',
					'src/**/*.js',
					'public/src/modules/translator.common.js',
					'public/src/modules/helpers.common.js',
					'public/src/utils.common.js',
					serverUpdated,
					'!src/upgrades/**',
				],
				options: {
					interval: interval,
				},
			},
			templatesUpdated: {
				files: [
					'src/views/**/*.tpl',
					...templatesUpdated,
				],
				options: {
					interval: interval,
				},
			},
			langUpdated: {
				files: [
					'public/language/en-GB/*.json',
					'public/language/en-GB/**/*.json',
					...langUpdated,
				],
				options: {
					interval: interval,
				},
			},
		});
		const build = require('./src/meta/build');
		if (!grunt.option('skip')) {
			await build.build(true, { watch: true });
		}
		run();
		done();
	});

	function run() {
		if (worker) {
			worker.kill();
		}

		const execArgv = [];
		const inspect = process.argv.find(a => a.startsWith('--inspect'));

		if (inspect) {
			execArgv.push(inspect);
		}

		worker = fork('app.js', args, {
			env,
			execArgv,
		});
	}

	grunt.task.run('init');

	grunt.event.removeAllListeners('watch');
	grunt.event.on('watch', (action, filepath, target) => {
		let compiling;
		if (target === 'styleUpdated') {
			compiling = ['clientCSS', 'acpCSS'];
		} else if (target === 'clientUpdated') {
			compiling = ['js'];
		} else if (target === 'templatesUpdated') {
			compiling = ['tpl'];
		} else if (target === 'langUpdated') {
			compiling = ['lang'];
		} else if (target === 'serverUpdated') {
			// empty require cache
			const paths = ['./src/meta/build.js', './src/meta/index.js'];
			paths.forEach(p => delete require.cache[require.resolve(p)]);
			return run();
		}

		require('./src/meta/build').build(compiling, { webpack: false }, (err) => {
			if (err) {
				winston.error(err.stack);
			}
			if (worker) {
				worker.send({ compiling: compiling });
			}
		});
	});
};

function addBaseThemes(pluginList) {
	let themeId = pluginList.find(p => p.includes('nodebb-theme-'));
	if (!themeId) {
		return pluginList;
	}
	let baseTheme;
	do {
		try {
			baseTheme = require(`${themeId}/theme`).baseTheme;
		} catch (err) {
			console.log(err);
		}

		if (baseTheme) {
			pluginList.push(baseTheme);
			themeId = baseTheme;
		}
	} while (baseTheme);
	return pluginList;
}
>>>>>>> 

### FILE: src/controllers/well-known.js
<<<<<<< 
module.exports = {
	'/.well-known/webfinger': async (req, res) => {
		const resource = req.query.resource;
		const uid = req.uid;

		if (!resource) {
			return res.status(400).send({ error: 'Missing resource parameter' });
		}

		const { hostname } = nconf.get('url_parsed');
		const expectedHostname = hostname.replace(/\/$/, '');
		const resourceHostname = resource.replace(/^acct:/, '').replace(/\/$/, '');

		if (resourceHostname !== expectedHostname) {
			return res.status(400).send({ error: 'Invalid resource parameter' });
		}

		const user = await db.getUserFromUsername(resource.replace(/^acct:/, ''));
		if (!user) {
			return res.status(404).send({ error: 'User not found' });
		}

		if (!await privileges.global.has(uid, 'groups:view:users')) {
			return res.status(403).send({ error: 'Forbidden' });
		}

		const links = [
			{
				rel: 'http://webfinger.net/rel/profile-page',
				type: 'text/html',
				href: `${nconf.get('base_url')}/user/${user.slug}`,
			},
		];

		const aliases = [
			`acct:${user.uid}`,
			`acct:${user.username}`,
			`acct:${user.slug}`,
		];

		const subject = resource;

		const response = {
			'links': links,
			'aliases': aliases,
			'subject': subject,
		};

		res.json(response);
	},
};
>>>>>>> 

### FILE: src/controllers/well-known.js
<<<<<<< 
module.exports = {
	'/.well-known/webfinger': async (req, res) => {
		const resource = req.query.resource;
		const uid = req.uid;

		if (!resource) {
			return res.status(400).send({ error: 'Missing resource parameter' });
		}

		const { hostname } = nconf.get('url_parsed');
		const expectedHostname = hostname.replace(/\/$/, '');
		const resourceHostname = resource.replace(/^acct:/, '').replace(/\/$/, '');

		if (resourceHostname !== expectedHostname) {
			return res.status(400).send({ error: 'Invalid resource parameter' });
		}

		const user = await db.getUserFromUsername(resource.replace(/^acct:/, ''));
		if (!user) {
			return res.status(404).send({ error: 'User not found' });
		}

		if (!await privileges.global.has(uid, 'groups:view:users')) {
			return res.status(403).send({ error: 'Forbidden' });
		}

		const links = [
			{
				rel: 'http://webfinger.net/rel/profile-page',
				type: 'text/html',
				href: `${nconf.get('base_url')}/user/${user.slug}`,
			},
		];

		const aliases = [
			`acct:${user.uid}`,
			`acct:${user.username}`,
			`acct:${user.slug}`,
		];

		const subject = resource;

		const response = {
			'links': links,
			'aliases': aliases,
			'subject': subject,
		};

		res.json(response);
	},
};
>>>>>>> 

### FILE: src/routes/well-known.js
<<<<<<< 
module.exports = {
	'/.well-known/change-password': async (req, res) => {
		res.redirect('/me/edit/password');
	},
};
>>>>>>> 

### FILE: src/routes/well-known.js
<<<<<<< 
module.exports = {
	'/.well-known/change-password': async (req, res) => {
		res.redirect('/me/edit/password');
	},
};
>>>>>>> 

### FILE: src/routes/well-known.js
<<<<<<< 
const wellKnownRouter = require('./well-known');

module.exports = function (router) {
	router.get('/webfinger', wellKnownRouter['/.well-known/webfinger']);
	router.get('/change-password', wellKnownRouter['/.well-known/change-password']);
};
>>>>>>> 

### FILE: src/routes/well-known.js
<<<<<<< 
const wellKnownRouter = require('./well-known');

module.exports = function (router) {
	router.get('/webfinger', wellKnownRouter['/.well-known/webfinger']);
	router.get('/change-password', wellKnownRouter['/.well-known/change-password']);
};
>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
const { paths } = require('./constants');
const wellKnownRouter = require('./src/routes/well-known');

>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
const { paths } = require('./constants');
const wellKnownRouter = require('./src/routes/well-known');

>>>>>>> 

### FILE: src/prestart.js
<<<<<<< 
function loadConfig(configFile) {
	nconf.file({
		file: configFile,
	});

	nconf.defaults({
		base_dir: paths.baseDir,
		themes_path: paths.nodeModules,
		upload_path: 'public/uploads',
		views_dir: path.join(paths.baseDir, 'build/public/templates'),
		version: pkg.version,
		isCluster: false,
		isPrimary: true,
		jobsDisabled: false,
		fontawesome: {
			pro: false,
			styles: '*',
		},
	});

	// Explicitly cast as Bool, loader.js passes in isCluster as string 'true'/'false'
	const castAsBool = ['isCluster', 'isPrimary', 'jobsDisabled'];
	nconf.stores.env.readOnly = false;
	castAsBool.forEach((prop) => {
		const value = nconf.get(prop);
		if (value !== undefined) {
			nconf.set(prop, ['1', 1, 'true', true].includes(value));
		}
	});
	nconf.stores.env.readOnly = true;
	nconf.set('runJobs', nconf.get('isPrimary') && !nconf.get('jobsDisabled'));

	// Ensure themes_path is a full filepath
	nconf.set('themes_path', path.resolve(paths.baseDir, nconf.get('themes_path')));
	nconf.set('core_templates_path', path.join(paths.baseDir, 'src/views'));

	nconf.set('upload_path', path.resolve(nconf.get('base_dir'), nconf.get('upload_path')));
	nconf.set('upload_url', '/assets/uploads');

	// nconf defaults, if not set in config
	if (!nconf.get('sessionKey')) {
		nconf.set('sessionKey', 'express.sid');
	}

	if (nconf.get('url')) {
		nconf.set('url', nconf.get('url').replace(/\/$/, ''));
		nconf.set('url_parsed', url.parse(nconf.get('url')));
		// Parse out the relative_url and other goodies from the configured URL
		const urlObject = url.parse(nconf.get('url'));
		const relativePath = urlObject.pathname !== '/' ? urlObject.pathname.replace(/\/+$/, '') : '';
		nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
		nconf.set('secure', urlObject.protocol === 'https:');
		nconf.set('use_port', !!urlObject.port);
		nconf.set('relative_path', relativePath);
		if (!nconf.get('asset_base_url')) {
			nconf.set('asset_base_url', `${relativePath}/assets`);
		}
		nconf.set('port', nconf.get('PORT') || nconf.get('port') || urlObject.port || (nconf.get('PORT_ENV_VAR') ? nconf.get(nconf.get('PORT_ENV_VAR')) : false) || 4567);

		// cookies don't provide isolation by port: http://stackoverflow.com/a/16328399/122353
		const domain = nconf.get('cookieDomain') || urlObject.hostname;
		const origins = nconf.get('socket.io:origins') || `${urlObject.protocol}//${domain}:*`;
		nconf.set('socket.io:origins', origins);
	}

	const app = express();
	app.use(express.static(path.join(paths.baseDir, 'build/public')));
	app.use('/assets/uploads', express.static(path.join(paths.baseDir, 'public/uploads')));
	app.use('/assets', express.static(path.join(paths.baseDir, 'build/public')));
	app.use('/api', express.static(path.join(paths.baseDir, 'build/public')));
	app.use('/api', express.static(path.join(paths.baseDir, 'public')));

	const router = express.Router();
	router.use('/api', require('./src/routes/api'));
	router.use('/api/admin', require('./src/routes/admin'));
	router.use('/api/v1', require('./src/routes/v1'));
	router.use('/api/v2', require('./src/routes/v2'));
	router.use('/api/v3', require('./src/routes/v3'));
	router.use('/api/v4', require('./src/routes/v4'));
	router.use('/api/v5', require('./src/routes/v5'));
	router.use('/api/v6', require('./src/routes/v6'));
	router.use('/api/v7', require('./src/routes/v7'));
	router.use('/api/v8', require('./src/routes/v8'));
	router.use('/api/v9', require('./src/routes/v9'));
	router.use('/api/v10', require('./src/routes/v10'));
	router.use('/api/v11', require('./src/routes/v11'));
	router.use('/api/v12', require('./src/routes/v12'));
	router.use('/api/v13', require('./src/routes/v13'));
	router.use('/api/v14', require('./src/routes/v14'));
	router.use('/api/v15', require('./src/routes/v15'));
	router.use('/api/v16', require('./src/routes/v16'));
	router.use('/api/v17', require('./src/routes/v17'));
	router.use('/api/v18', require('./src/routes/v18'));
	router.use('/api/v19', require('./src/routes/v19'));
	router.use('/api/v20', require('./src/routes/v20'));
	router.use('/api/v21', require('./src/routes/v21'));
	router.use('/api/v22', require('./src/routes/v22'));
	router.use('/api/v23', require('./src/routes/v23'));
	router.use('/api/v24', require('./src/routes/v24'));
	router.use('/api/v25', require('./src/routes/v25'));
	router.use('/api/v26', require('./src/routes/v26'));
	router.use('/api/v27', require('./src/routes/v27'));
	router.use('/api/v28', require('./src/routes/v28'));
	router.use('/api/v29', require('./src/routes/v29'));
	router.use('/api/v30', require('./src/routes/v30'));
	router.use('/api/v31', require('./src/routes/v31'));
	router.use('/api/v32', require('./src/routes/v32'));
	router.use('/api/v33', require('./src/routes/v33'));
	router.use('/api/v34', require('./src/routes/v34'));
	router.use('/api/v35', require('./src/routes/v35'));
	router.use('/api/v36', require('./src/routes/v36'));
	router.use('/api/v37', require('./src/routes/v37'));
	router.use('/api/v38', require('./src/routes/v38'));
	router.use('/api/v39', require('./src/routes/v39'));
	router.use('/api/v40', require('./src/routes/v40'));
	router.use('/api/v41', require('./src/routes/v41'));
	router.use('/api/v42', require('./src/routes/v42'));
	router.use('/api/v43', require('./src/routes/v43'));
	router.use('/api/v44', require('./src/routes/v44'));
	router.use('/api/v45', require('./src/routes/v45'));
	router.use('/api/v46', require('./src/routes/v46'));
	router.use('/api/v47', require('./src/routes/v47'));
	router.use('/api/v48', require('./src/routes/v48'));
	router.use('/api/v49', require('./src/routes/v49'));
	router.use('/api/v50', require('./src/routes/v50'));
	router.use('/api/v51', require('./src/routes/v51'));
	router.use('/api/v52', require('./src/routes/v52'));
	router.use('/api/v53', require('./src/routes/v53'));
	router.use('/api/v54', require('./src/routes/v54'));
	router.use('/api/v55', require('./src/routes/v55'));
	router.use('/api/v56', require('./src/routes/v56'));
	router.use('/api/v57', require('./src/routes/v57'));
	router.use('/api/v58', require('./src/routes/v58'));
	router.use('/api/v59', require('./src/routes/v59'));
	router.use('/api/v60', require('./src/routes/v60'));
	router.use('/api/v61', require('./src/routes/v61'));
	router.use('/api/v62', require('./src/routes/v62'));
	router.use('/api/v63', require('./src/routes/v63'));
	router.use('/api/v64', require('./src/routes/v64'));
	router.use('/api/v65', require('./src/routes/v65'));
	router.use('/api/v66', require('./src/routes/v66'));
	router.use('/api/v67', require('./src/routes/v67'));
	router.use('/api/v68', require('./src/routes/v68'));
	router.use('/api/v69', require('./src/routes/v69'));
	router.use('/api/v70', require('./src/routes/v70'));
	router.use('/api/v71', require('./src/routes/v71'));
	router.use('/api/v72', require('./src/routes/v72'));
	router.use('/api/v73', require('./src/routes/v73'));
	router.use('/api/v74', require('./src/routes/v74'));
	router.use('/api/v75', require('./src/routes/v75'));
	router.use('/api/v76', require('./src/routes/v76'));
	router.use('/api/v77', require('./src/routes/v77'));
	router.use('/api/v78', require('./src/routes/v78'));
	router.use('/api/v79', require('./src/routes/v79'));
	router.use('/api/v80', require('./src/routes/v80'));
	router.use('/api/v81', require('./src/routes/v81'));
	router.use('/api/v82', require('./src/routes/v82'));
	router.use('/api/v83', require('./src/routes/v83'));
	router.use('/api/v84', require('./src/routes/v84'));
	router.use('/api/v85', require('./src/routes/v85'));
	router.use('/api/v86', require('./src/routes/v86'));
	router.use('/api/v87', require('./src/routes/v87'));
	router.use('/api/v88', require('./src/routes/v88'));
	router.use('/api/v89', require('./src/routes/v89'));
	router.use('/api/v90', require('./src/routes/v90'));
	router.use('/api/v91', require('./src/routes/v91'));
	router.use('/api/v92', require('./src/routes/v92'));
	router.use('/api/v93', require('./src/routes/v93'));
	router.use('/api/v94', require('./src/routes/v94'));
	router.use('/api/v95', require('./src/routes/v95'));
	router.use('/api/v96', require('./src/routes/v96'));
	router.use('/api/v97', require('./src/routes/v97'));
	router.use('/api/v98', require('./src/routes/v98'));
	router.use('/api/v99', require('./src/routes/v99'));
	router.use('/api/v100', require('./src/routes/v100'));
	router.use('/api/v101', require('./src/routes/v101'));
	router.use('/api/v102', require('./src/routes/v102'));
	router.use('/api/v103', require('./src/routes/v103'));
	router.use('/api/v104', require('./src/routes/v104'));
	router.use('/api/v105', require('./src/routes/v105'));
	router.use('/api/v106', require('./src/routes/v106'));
	router.use('/api/v107', require('./src/routes/v107'));
	router.use('/api/v108', require('./src/routes/v108'));
	router.use('/api/v109', require('./src/routes/v109'));
	router.use('/api/v110', require('./src/routes/v110'));
	router.use('/api/v111', require('./src/routes/v111'));
	router.use('/api/v112', require('./src/routes/v112'));
	router.use('/api/v113', require('./src/routes/v113'));
	router.use('/api/v114', require('./src/routes/v114'));
	router.use('/api/v115', require('./src/routes/v115'));
	router.use('/api/v116', require('./src/routes/v116'));
	router.use('/api/v117', require('./src/routes/v117'));
	router.use('/api/v118', require('./src/routes/v118'));
	router.use('/api/v119', require('./src/routes/v119'));
	router.use('/api/v120', require('./src/routes/v120'));
	router.use('/api/v121', require('./src/routes/v121'));
	router.use('/api/v122', require('./src/routes/v122'));
	router.use('/api/v123', require('./src/routes/v123'));
	router.use('/api/v124', require('./src/routes/v124'));
	router.use('/api/v125', require('./src/routes/v125'));
	router.use('/api/v126', require('./src/routes/v126'));
	router.use('/api/v127', require('./src/routes/v127'));
	router.use('/api/v128', require('./src/routes/v128'));
	router.use('/api/v129', require('./src/routes/v129'));
	router.use('/api/v130', require('./src/routes/v130'));
	router.use('/api/v131', require('./src/routes/v131'));
	router.use('/api/v132', require('./src/routes/v132'));
	router.use('/api/v133', require('./src/routes/v133'));
	router.use('/api/v134', require('./src/routes/v134'));
	router.use('/api/v135', require('./src/routes/v135'));
	router.use('/api/v136', require('./src/routes/v136'));
	router.use('/api/v137', require('./src/routes/v137'));
	router.use('/api/v138', require('./src/routes/v138'));
	router.use('/api/v139', require('./src/routes/v139'));
	router.use('/api/v140', require('./src/routes/v140'));
	router.use('/api/v141', require('./src/routes/v141'));
	router.use('/api/v142', require('./src/routes/v142'));
	router.use('/api/v143', require('./src/routes/v143'));
	router.use('/api/v144', require('./src/routes/v144'));
	router.use('/api/v145', require('./src/routes/v145'));
	router.use('/api/v146', require('./src/routes/v146'));
	router.use('/api/v147', require('./src/routes/v147'));
	router.use('/api/v148', require('./src/routes/v148'));
	router.use('/api/v149', require('./src/routes/v149'));
	router.use('/api/v150', require('./src/routes/v150'));
	router.use('/api/v151', require('./src/routes/v151'));
	router.use('/api/v152', require('./src/routes/v152'));
	router.use('/api/v153', require('./src/routes/v153'));
	router.use('/api/v154', require('./src/routes/v154'));
	router.use('/api/v155', require('./src/routes/v155'));
	router.use('/api/v156', require('./src/routes/v156'));
	router.use('/api/v157', require('./src/routes/v157'));
	router.use('/api/v158', require('./src/routes/v158'));
	router.use('/api/v159', require('./src/routes/v159'));
	router.use('/api/v160', require('./src/routes/v160'));
	router.use('/api/v161', require('./src/routes/v161'));
	router.use('/api/v162', require('./src/routes/v162'));
	router.use('/api/v163', require('./src/routes/v163'));
	router.use('/api/v164', require('./src/routes/v164'));
	router.use('/api/v165', require('./src/routes/v165'));
	router.use('/api/v166', require('./src/routes/v166'));
	router.use('/api/v167', require('./src/routes/v167'));
	router.use('/api/v168', require('./src/routes/v168'));
	router.use('/api/v169', require('./src/routes/v169'));
	router.use('/api/v170', require('./src/routes/v170'));
	router.use('/api/v171', require('./src/routes/v171'));
	router.use('/api/v172', require('./src/routes/v172'));
	router.use('/api/v173', require('./src/routes/v173'));
	router.use('/api/v174', require('./src/routes/v174'));
	router.use('/api/v175', require('./src/routes/v175'));
	router.use('/api/v176', require('./src/routes/v176'));
	router.use('/api/v177', require('./src/routes/v177'));
	router.use('/api/v178', require('./src/routes/v178'));
	router.use('/api/v179', require('./src/routes/v179'));
	router.use('/api/v180', require('./src/routes/v180'));
	router.use('/api/v181', require('./src/routes/v181'));
	router.use('/api/v182', require('./src/routes/v182'));
	router.use('/api/v183', require('./src/routes/v183'));
	router.use('/api/v184', require('./src/routes/v184'));
	router.use('/api/v185', require('./src/routes/v185'));
	router.use('/api/v186', require('./src/routes/v186'));
	router.use('/api/v187', require('./src/routes/v187'));
	router.use('/api/v188', require('./src/routes/v188'));
	router.use('/api/v189', require('./src/routes/v189'));
	router.use('/api/v190', require('./src/routes/v190'));
	router.use('/api/v191', require('./src/routes/v191'));
	router.use('/api/v192', require('./src/routes/v192'));
	router.use('/api/v193', require('./src/routes/v193'));
	router.use('/api/v194', require('./src/routes/v194'));
	router.use('/api/v195', require('./src/routes/v195'));
	router.use('/api/v196', require('./src/routes/v196'));
	router.use('/api/v197', require('./src/routes/v197'));
	router.use('/api/v198', require('./src/routes/v198'));
	router.use('/api/v199', require('./src/routes/v199'));
	router.use('/api/v200', require('./src/routes/v200'));
	router.use('/api/v201', require('./src/routes/v201'));
	router.use('/api/v202', require('./src/routes/v202'));
	router.use('/api/v203', require('./src/routes/v203'));
	router.use('/api/v204', require('./src/routes/v204'));
	router.use('/api/v205', require('./src/routes/v205'));
	router.use('/api/v206', require('./src/routes/v206'));
	router.use('/api/v207', require('./src/routes/v207'));
	router.use('/api/v208', require('./src/routes/v208'));
	router.use('/api/v209', require('./src/routes/v209'));
	router.use('/api/v210', require('./src/routes/v210'));
	router.use('/api/v211', require('./src/routes/v211'));
	router.use('/api/v212', require('./src/routes/v212'));
	router.use('/api/v213', require('./src/routes/v213'));
	router.use('/api/v214', require('./src/routes/v214'));
	router.use('/api/v215', require('./src/routes/v215'));
	router.use('/api/v216', require('./src/routes/v216'));
	router.use('/api/v217', require('./src/routes/v217'));
	router.use('/api/v218', require('./src/routes/v218'));
	router.use('/api/v219', require('./src/routes/v219'));
	router.use('/api/v220', require('./src/routes/v220'));
	router.use('/api/v221', require('./src/routes/v221'));
	router.use('/api/v222', require('./src/routes/v222'));
	router.use('/api/v223', require('./src/routes/v223'));
	router.use('/api/v224', require('./src/routes/v224'));
	router.use('/api/v225', require('./src/routes/v225'));
	router.use('/api/v226', require('./src/routes/v226'));
	router.use('/api/v227', require('./src/routes/v227'));
	router.use('/api/v228', require('./src/routes/v228'));
	router.use('/api/v229', require('./src/routes/v229'));
	router.use('/api/v230', require('./src/routes/v230'));
	router.use('/api/v231', require('./src/routes/v231'));
	router.use('/api/v232', require('./src/routes/v232'));
	router.use('/api/v233', require('./src/routes/v233'));
	router.use('/api/v234', require('./src/routes/v234'));
	router.use('/api/v235', require('./src/routes/v235'));
	router.use('/api/v236', require('./src/routes/v236'));
	router.use('/api/v237', require('./src/routes/v237'));
	router.use('/api/v238', require('./src/routes/v238'));
	router.use('/api/v239', require('./src/routes/v239'));
	router.use('/api/v240', require('./src/routes/v240'));
	router.use('/api/v241', require('./src/routes/v241'));
	router.use('/api/v242', require('./src/routes/v242'));
	router.use('/api/v243', require('./src/routes/v243'));
	router.use('/api/v244', require('./src/routes/v244'));
	router.use('/api/v245', require('./src/routes/v245'));
	router.use('/api/v246', require('./src/routes/v246'));
	router.use('/api/v247', require('./src/routes/v247'));
	router.use('/api/v248', require('./src/routes/v248'));
	router.use('/api/v249', require('./src/routes/v249'));
	router.use('/api/v250', require('./src/routes/v250'));
	router.use('/api/v251', require('./src/routes/v251'));
	router.use('/api/v252', require('./src/routes/v252'));
	router.use('/api/v253', require('./src/routes/v253'));
	router.use('/api/v254', require('./src/routes/v254'));
	router.use('/api/v255', require('./src/routes/v255'));
	router.use('/api/v256', require('./src/routes/v256'));
	router.use('/api/v257', require('./src/routes/v257'));
	router.use('/api/v258', require('./src/routes/v258'));
	router.use('/api/v259', require('./src/routes/v259'));
	router.use('/api/v260', require('./src/routes/v260'));
	router.use('/api/v261', require('./src/routes/v261'));
	router.use('/api/v262', require('./src/routes/v262'));
	router.use('/api/v263', require('./src/routes/v263'));
	router.use('/api/v264', require('./src/routes/v264'));
	router.use('/api/v265', require('./src/routes/v265'));
	router.use('/api/v266', require('./src/routes/v266'));
	router.use('/api/v267', require('./src/routes/v267'));
	router.use('/api/v268', require('./src/routes/v268'));
	router.use('/api/v269', require('./src/routes/v269'));
	router.use('/api/v270', require('./src/routes/v270'));
	router.use('/api/v271', require('./src/routes/v271'));
	router.use('/api/v272', require('./src/routes/v272'));
	router.use('/api/v273', require('./src/routes/v273'));
	router.use('/api/v274', require('./src/routes/v274'));
	router.use('/api/v275', require('./src/routes/v275'));
	router.use('/api/v276', require('./src/routes/v276'));
	router.use('/api/v277', require('./src/routes/v277'));
	router.use('/api/v278', require('./src/routes/v278'));
	router.use('/api/v279', require('./src/routes/v279'));
	router.use('/api/v280', require('./src/routes/v280'));
	router.use('/api/v281', require('./src/routes/v281'));
	router.use('/api/v282', require('./src/routes/v282'));
	router.use('/api/v283', require('./src/routes/v283'));
	router.use('/api/v284', require('./src/routes/v284'));
	router.use('/api/v285', require('./src/routes/v285'));
	router.use('/api/v286', require('./src/routes/v286'));
	router.use('/api/v287', require('./src/routes/v287'));
	router.use('/api/v288', require('./src/routes/v288'));
	router.use('/api/v289', require('./src/routes/v289'));
	router.use('/api/v290', require('./src/routes/v290'));
	router.use('/api/v291', require('./src/routes/v291'));
	router.use('/api/v292', require('./src/routes/v292'));
	router.use('/api/v293', require('./src/routes/v293'));
	router.use('/api/v294', require('./src/routes/v294'));
	router.use('/api/v295', require('./src/routes/v295'));
	router.use('/api/v296', require('./src/routes/v296'));
	router.use('/api/v297', require('./src/routes/v297'));
	router.use('/api/v298', require('./src/routes/v298'));
	router.use('/api/v299', require('./src/routes/v299'));
	router.use('/api/v300', require('./src/routes/v300'));
	router.use('/api/v301', require('./src/routes/v301'));
	router.use('/api/v302', require('./src/routes/v302'));
	router.use('/api/v303', require('./src/routes/v303'));
	router.use('/api/v304', require('./src/routes/v304'));
	router.use('/api/v305', require('./src/routes/v305'));
	router.use('/api/v306', require('./src/routes/v306'));
	router.use('/api/v307', require('./src/routes/v307'));
	router.use('/api/v308', require('./src/routes/v308'));
	router.use('/api/v309', require('./src/routes/v309'));
	router.use('/api/v310', require('./src/routes/v310'));
	router.use('/api/v311', require('./src/routes/v311'));
	router.use('/api/v312', require('./src/routes/v312'));
	router.use('/api/v313', require('./src/routes/v313'));
	router.use('/api/v314', require('./src/routes/v314'));
	router.use('/api/v315', require('./src/routes/v315'));
	router.use('/api/v316', require('./src/routes/v316'));
	router.use('/api/v317', require('./src/routes/v317'));
	router.use('/api/v318', require('./src/routes/v318'));
	router.use('/api/v319', require('./src/routes/v319'));
	router.use('/api/v320', require('./src/routes/v320'));
	router.use('/api/v321', require('./src/routes/v321'));
	router.use('/api/v322', require('./src/routes/v322'));
	router.use('/api/v323', require('./src/routes/v323'));
	router.use('/api/v324', require('./src/routes/v324'));
	router.use('/api/v325', require('./src/routes/v325'));
	router.use('/api/v326', require('./src/routes/v326'));
	router.use('/api/v327', require('./src/routes/v327'));
	router.use('/api/v328', require('./src/routes/v328'));
	router.use('/api/v329', require('./src/routes/v329'));
	router.use('/api/v330', require('./src/routes/v330'));
	router.use('/api/v331', require('./src/routes/v331'));
	router.use('/api/v332', require('./src/routes/v332'));
	router.use('/api/v333', require('./src/routes/v333'));
	router.use('/api/v334', require('./src/routes/v334'));
	router.use('/api/v335', require('./src/routes/v335'));
	router.use('/api/v336', require('./src/routes/v336'));
	router.use('/api/v337', require('./src/routes/v337'));
	router.use('/api/v338', require('./src/routes/v338'));
	router.use('/api/v339', require('./src/routes/v339'));
	router.use('/api/v340', require('./src/routes/v340'));
	router.use('/api/v341', require('./src/routes/v341'));
	router.use('/api/v342', require('./src/routes/v342'));
	router.use('/api/v343', require('./src/routes/v343'));
	router.use('/api/v344', require('./src/routes/v344'));
	router.use('/api/v345', require('./src/routes/v345'));
	router.use('/api/v346', require('./src/routes/v346'));
	router.use('/api/v347', require('./src/routes/v347'));
	router.use('/api/v348', require('./src/routes/v348'));
	router.use('/api/v349', require('./src/routes/v349'));
	router.use('/api/v350', require('./src/routes/v350'));
	router.use('/api/v351', require('./src/routes/v351'));
	router.use('/api/v352', require('./src/routes/v352'));
	router.use('/api/v353', require('./src/routes/v353'));
	router.use('/api/v354', require('./src/routes/v354'));
	router.use('/api/v355', require('./src/routes/v355'));
	router.use('/api/v356', require('./src/routes/v356'));
	router.use('/api/v357', require('./src/routes/v357'));
	router.use('/api/v358', require('./src/routes/v358'));
	router.use('/api/v359', require('./src/routes/v359'));
	router.use('/api/v360', require('./src/routes/v360'));
	router.use('/api/v361', require('./src/routes/v361'));
	router.use('/api/v362', require('./src/routes/v362'));
	router.use('/api/v363', require('./src/routes/v363'));
	router.use('/api/v364', require('./src/routes/v364'));
	router.use('/api/v365', require('./src/routes/v365'));
	router.use('/api/v366', require('./src/routes/v366'));
	router.use('/api/v367', require('./src/routes/v367'));
	router.use('/api/v368', require('./src/routes/v368'));
	router.use('/api/v369', require('./src/routes/v369'));
	router.use('/api/v370', require('./src/routes/v370'));
	router.use('/api/v371', require('./src/routes/v371'));
	router.use('/api/v372', require('./src/routes/v372'));
	router.use('/api/v373', require('./src/routes/v373'));
	router.use('/api/v374', require('./src/routes/v374'));
	router.use('/api/v375', require('./src/routes/v375'));
	router.use('/api/v376', require('./src/routes/v376'));
	router.use('/api/v377', require('./src/routes/v377'));
	router.use('/api/v378', require('./src/routes/v378'));
	router.use('/api/v379', require('./src/routes/v379'));
	router.use('/api/v380', require('./src/routes/v380'));
	router.use('/api/v381', require('./src/routes/v381'));
	router.use('/api/v382', require('./src/routes/v382'));
	router.use('/api/v383', require('./src/routes/v383'));
	router.use('/api/v384', require('./src/routes/v384'));
	router.use('/api/v385', require('./src/routes/v385'));
	router.use('/api/v386', require('./src/routes/v386'));
	router.use('/api/v387', require('./src/routes/v387'));
	router.use('/api/v388', require('./src/routes/v388'));
	router.use('/api/v389', require('./src/routes/v389'));
	router.use('/api/v390', require('./src/routes/v390'));
	router.use('/api/v391', require('./src/routes/v391'));
	router.use('/api/v392', require('./src/routes/v392'));
	router.use('/api/v393', require('./src/routes/v393'));
	router.use('/api/v394', require('./src/routes/v394'));
	router.use('/api/v395', require('./src/routes/v395'));
	router.use('/api/v396', require('./src/routes/v396'));
	router.use('/api/v397', require('./src/routes/v397'));
	router.use('/api/v398', require('./src/routes/v398'));
	router.use('/api/v399', require('./src/routes/v399'));
	router.use('/api/v400', require('./src/routes/v400'));
	router.use('/api/v401', require('./src/routes/v401'));
	router.use('/api/v402', require('./src/routes/v402'));
	router.use('/api/v403', require('./src/routes/v403'));
	router.use('/api/v404', require('./src/routes/v404'));
	router.use('/api/v405', require('./src/routes/v405'));
	router.use('/api/v406', require('./src/routes/v406'));
	router.use('/api/v407', require('./src/routes/v407'));
	router.use('/api/v408', require('./src/routes/v408'));
	router.use('/api/v409', require('./src/routes/v409'));
	router.use('/api/v410', require('./src/routes/v410'));
	router.use('/api/v411', require('./src/routes/v411'));
	router.use('/api/v412', require('./src/routes/v412'));
	router.use('/api/v413', require('./src/routes/v413'));
	router.use('/api/v414', require('./src/routes/v414'));
	router.use('/api/v415', require('./src/routes/v415'));
	router.use('/api/v416', require('./src/routes/v416'));
	router.use('/api/v417', require('./src/routes/v417'));
	router.use('/api/v418', require('./src/routes/v418'));
	router.use('/api/v419', require('./src/routes/v419'));
	router.use('/api/v420', require('./src/routes/v420'));
	router.use('/api/v421', require('./src/routes/v421'));
	router.use('/api/v422', require('./src/routes/v422'));
	router.use('/api/v423', require('./src/routes/v423'));
	router.use('/api/v424', require('./src/routes/v424'));
	router.use('/api/v425', require('./src/routes/v425'));
	router.use('/api/v426', require('./src/routes/v426'));
	router.use('/api/v427', require('./src/routes/v427'));
	router.use('/api/v428', require('./src/routes/v428'));
	router.use('/api/v429', require('./src/routes/v429'));
	router.use('/api/v430', require('./src/routes/v430'));
	router.use('/api/v431', require('./src/routes/v431'));
	router.use('/api/v432', require('./src/routes/v432'));
	router.use('/api/v433', require('./src/routes/v433'));
	router.use('/api/v434', require('./src/routes/v434'));
	router.use('/api/v435', require('./src/routes/v435'));
	router.use('/api/v436', require('./src/routes/v436'));
	router.use('/api/v437', require('./src/routes/v437'));
	router.use('/api/v438', require('./src/routes/v438'));
	router.use('/api/v439', require('./src/routes/v439'));
	router.use('/api/v440', require('./src/routes/v440'));
	router.use('/api/v441', require('./src/routes/v441'));
	router.use('/api/v442', require('./src/routes/v442'));
	router.use('/api/v443', require('./src/routes/v443'));
	router.use('/api/v444', require('./src/routes/v444'));
	router.use('/api/v445', require('./src/routes/v445'));
	router.use('/api/v446', require('./src/routes/v446'));
	router.use('/api/v447', require('./src/routes/v447'));
	router.use('/api/v448', require('./src/routes/v448'));
	router.use('/api/v449', require('./src/routes/v449'));
	router.use('/api/v450', require('./src/routes/v450'));
	router.use('/api/v451', require('./src/routes/v451'));
	router.use('/api/v452', require('./src/routes/v452'));
	router.use('/api/v453', require('./src/routes/v453'));
	router.use('/api/v454', require('./src/routes/v454'));
	router.use('/api/v455', require('./src/routes/v455'));
	router.use('/api/v456', require('./src/routes/v456'));
	router.use('/api/v457', require('./src/routes/v457'));
	router.use('/api/v458', require('./src/routes/v458'));
	router.use('/api/v459', require('./src/routes/v459'));
	router.use('/api/v460', require('./src/routes/v460'));
	router.use('/api/v461', require('./src/routes/v461'));
	router.use('/api/v462', require('./src/routes/v462'));
	router.use('/api/v463', require('./src/routes/v463'));
	router.use('/api/v464', require('./src/routes/v464'));
	router.use('/api/v465', require('./src/routes/v465'));
	router.use('/api/v466', require('./src/routes/v466'));
	router.use('/api/v467', require('./src/routes/v467'));
	router.use('/api/v468', require('./src/routes/v468'));
	router.use('/api/v469', require('./src/routes/v469'));
	router.use('/api/v470', require('./src/routes/v470'));
	router.use('/api/v471', require('./src/routes/v471'));
	router.use('/api/v472', require('./src/routes/v472'));
	router.use('/api/v473', require('./src/routes/v473'));
	router.use('/api/v474', require('./src/routes/v474'));
	router.use('/api/v475', require('./src/routes/v475'));
	router.use('/api/v476', require('./src/routes/v476'));
	router.use('/api/v477', require('./src/routes/v477'));
	router.use('/api/v478', require('./src/routes/v478'));
	router.use('/api/v479', require('./src/routes/v479'));
	router.use('/api/v480', require('./src/routes/v480'));
	router.use('/api/v481', require('./src/routes/v481'));
	router.use('/api/v482', require('./src/routes/v482'));
	router.use('/api/v483', require('./src/routes/v483'));
	router.use('/api/v484', require('./src/routes/v484'));
	router.use('/api/v485', require('./src/routes/v485'));
	router.use('/api/v486', require('./src/routes/v486'));
	router.use('/api/v487', require('./src/routes/v487'));
	router.use('/api/v488', require('./src/routes/v488'));
	router.use('/api/v489', require('./src/routes/v489'));
	router.use('/api/v490', require('./src/routes/v490'));
	router.use('/api/v491', require('./src/routes/v491'));
	router.use('/api/v492', require('./src/routes/v492'));
	router.use('/api/v493', require('./src/routes/v493'));
	router.use('/api/v494', require('./src/routes/v494'));
	router.use('/api/v495', require('./src/routes/v495'));
	router.use('/api/v496', require('./src/routes/v496'));
	router.use('/api/v497', require('./src/routes/v497'));
	router.use('/api/v498', require('./src/routes/v498'));
	router.use('/api/v499', require('./src/routes/v499'));
	router.use('/api/v500', require('./src/routes/v500'));
	router.use('/api/v501', require('./src/routes/v501'));
	router.use('/api/v502', require('./src/routes/v502'));
	router.use('/api/v503', require('./src/routes/v503'));
	router.use('/api/v504', require('./src/routes/v504'));
	router.use('/api/v505', require('./src/routes/v505'));
	router.use('/api/v506', require('./src/routes/v506'));
	router.use('/api/v507', require('./src/routes/v507'));
	router.use('/api/v508', require('./src/routes/v508'));
	router.use('/api/v509', require('./src/routes/v509'));
	router.use('/api/v510', require('./src/routes/v510'));
	router.use('/api/v511', require('./src/routes/v511'));
	router.use('/api/v512', require('./src/routes/v512'));
	router.use('/api/v513', require('./src/routes/v513'));
	router.use('/api/v514', require('./src/routes/v514'));
	router.use('/api/v515', require('./src/routes/v515'));
	router.use('/api/v516', require('./src/routes/v516'));
	router.use('/api/v517', require('./src/routes/v517'));
	router.use('/api/v518', require('./src/routes/v518'));
	router.use('/api/v519', require('./src/routes/v519'));
	router.use('/api/v520', require('./src/routes/v520'));
	router.use('/api/v521', require('./src/routes/v521'));
	router.use('/api/v522', require('./src/routes/v522'));
	router.use('/api/v523', require('./src/routes/v523'));
	router.use('/api/v524', require('./src/routes/v524'));
	router.use('/api/v525', require('./src/routes/v525'));
	router.use('/api/v526', require('./src/routes/v526'));
	router.use('/api/v527', require('./src/routes/v527'));
	router.use('/api/v528', require('./src/routes/v528'));
	router.use('/api/v529', require('./src/routes/v529'));
	router.use('/api/v530', require('./src/routes/v530'));
	router.use('/api/v531', require('./src/routes/v531'));
	router.use('/api/v532', require('./src/routes/v532'));
	router.use('/api/v533', require('./src/routes/v533'));
	router.use('/api/v534', require('./src/routes/v534'));
	router.use('/api/v535', require('./src/routes/v535'));
	router.use('/api/v536', require('./src/routes/v536'));
	router.use('/api/v537', require('./src/routes/v537'));
	router.use('/api/v538', require('./src/routes/v538'));
	router.use('/api/v539', require('./src/routes/v539'));
	router.use('/api/v540', require('./src/routes/v540'));
	router.use('/api/v541', require('./src/routes/v541'));
	router.use('/api/v542', require('./src/routes/v542'));
	router.use('/api/v543', require('./src/routes/v543'));
	router.use('/api/v544', require('./src/routes/v544'));
	router.use('/api/v545', require('./src/routes/v545'));
	router.use('/api/v546', require('./src/routes/v546'));
	router.use('/api/v547', require('./src/routes/v547'));
	router.use('/api/v548', require('./src/routes/v548'));
	router.use('/api/v549', require('./src/routes/v549'));
	router.use('/api/v550', require('./src/routes/v550'));
	router.use('/api/v551', require('./src/routes/v551'));
	router.use('/api/v552', require('./src/routes/v552'));
	router.use('/api/v553', require('./src/routes/v553'));
	router.use('/api/v554', require('./src/routes/v554'));
	router.use('/api/v555', require('./src/routes/v555'));
	router.use('/api/v556', require('./src/routes/v556'));
	router.use('/api/v557', require('./src/routes/v557'));
	router.use('/api/v558', require('./src/routes/v558'));
	router.use('/api/v559', require('./src/routes/v559'));
	router.use('/api/v560', require('./src/routes/v560'));
	router.use('/api/v561', require('./src/routes/v561'));
	router.use('/api/v562', require('./src/routes/v562'));
	router.use('/api/v563', require('./src/routes/v563'));
	router.use('/api/v564', require('./src/routes/v564'));
	router.use('/api/v565', require('./src/routes/v565'));
	router.use('/api/v566', require('./src/routes/v566'));
	router.use('/api/v567', require('./src/routes/v567'));
	router.use('/api/v568', require('./src/routes/v568'));
	router.use('/api/v569', require('./src/routes/v569'));
	router.use('/api/v570', require('./src/routes/v570'));
	router.use('/api/v571', require('./src/routes/v571'));
	router.use('/api/v572', require('./src/routes/v572'));
	router.use('/api/v573', require('./src/routes/v573'));
	router.use('/api/v574', require('./src/routes/v574'));
	router.use('/api/v575', require('./src/routes/v575'));
	router.use('/api/v576', require('./src/routes/v576'));
	router.use('/api/v577', require('./src/routes/v577'));
	router.use('/api/v578', require('./src/routes/v578'));
	router.use('/api/v579', require('./src/routes/v579'));
	router.use('/api/v580', require('./src/routes/v580'));
	router.use('/api/v581', require('./src/routes/v581'));
	router.use('/api/v582', require('./src/routes/v582'));
	router.use('/api/v583', require('./src/routes/v583'));
	router.use('/api/v584', require('./src/routes/v584'));
	router.use('/api/v585', require('./src/routes/v585'));
	router.use('/api/v586', require('./src/routes/v586'));
	router.use('/api/v587', require('./src/routes/v587'));
	router.use('/api/v588', require('./src/routes/v588'));
	router.use('/api/v589', require('./src/routes/v589'));
	router.use('/api/v590', require('./src/routes/v590'));
	router.use('/api/v591', require('./src/routes/v591'));
	router.use('/api/v592', require('./src/routes/v592'));
	router.use('/api/v593', require('./src/routes/v593'));
	router.use('/api/v594', require('./src/routes/v594'));
	router.use('/api/v595', require('./src/routes/v595'));
	router.use('/api/v596', require('./src/routes/v596'));
	router.use('/api/v597', require('./src/routes/v597'));
	router.use('/api/v598', require('./src/routes/v598'));
	router.use('/api/v599', require('./src/routes/v599'));
	router.use('/api/v600', require('./src/routes/v600'));
	router.use('/api/v601', require('./src/routes/v601'));
	router.use('/api/v602', require('./src/routes/v602'));
	router.use('/api/v603', require('./src/routes/v603'));
	router.use('/api/v604', require('./src/routes/v604'));
	router.use('/api/v605', require('./src/routes/v605'));
	router.use('/api/v606', require('./src/routes/v606'));
	router.use('/api/v607', require('./src/routes/v607'));
	router.use('/api/v608', require('./src/routes/v608'));
	router.use('/api/v609', require('./src/routes/v609'));
	router.use('/api/v610', require('./src/routes/v610'));
	router.use('/api/v611', require('./src/routes/v611'));
	router.use('/api/v612', require('./src/routes/v612'));
	router.use('/api/v613', require('./src/routes/v613'));
	router.use('/api/v614', require('./src/routes/v614'));
	router.use('/api/v615', require('./src/routes/v615'));
	router.use('/api/v616', require('./src/routes/v616'));
	router.use('/api/v617', require('./src/routes/v617'));
	router.use('/api/v618', require('./src/routes/v618'));
	router.use('/api/v619', require('./src/routes/v619'));
	router.use('/api/v620', require('./src/routes/v620'));
	router.use('/api/v621', require('./src/routes/v621'));
	router.use('/api/v622', require('./src/routes/v622'));
	router.use('/api/v623', require('./src/routes/v623'));
	router.use('/api/v624', require('./src/routes/v624'));
	router.use('/api/v625', require('./src/routes/v625'));
	router.use('/api/v626', require('./src/routes/v626'));
	router.use('/api/v627', require('./src/routes/v627'));
	router.use('/api/v628', require('./src/routes/v628'));
	router.use('/api/v629', require('./src/routes/v629'));
	router.use('/api/v630', require('./src/routes/v630'));
	router.use('/api/v631', require('./src/routes/v631'));
	router.use('/api/v632', require('./src/routes/v632'));
	router.use('/api/v633', require('./src/routes/v633'));
	router.use('/api/v634', require('./src/routes/v634'));
	router.use('/api/v635', require('./src/routes/v635'));
	router.use('/api/v636', require('./src/routes/v636'));
	router.use('/api/v637', require('./src/routes/v637'));
	router.use('/api/v638', require('./src/routes/v638'));
	router.use('/api/v639', require('./src/routes/v639'));
	router.use('/api/v640', require('./src/routes/v640'));
	router.use('/api/v641', require('./src/routes/v641'));
	router.use('/api/v642', require('./src/routes/v642'));
	router.use('/api/v643', require('./src/routes/v643'));
	router.use('/api/v644', require('./src/routes/v644'));
	router.use('/api/v645', require('./src/routes/v645'));
	router.use('/api/v646', require('./src/routes/v646'));
	router.use('/api/v647', require('./src/routes/v647'));
	router.use('/api/v648', require('./src/routes/v648'));
	router.use('/api/v649', require('./src/routes/v649'));
	router.use('/api/v650', require('./src/routes/v650'));
	router.use('/api/v651', require('./src/routes/v651'));
	router.use('/api/v652', require('./src/routes/v652'));
	router.use('/api/v653', require('./src/routes/v653'));
	router.use('/api/v654', require('./src/routes/v654'));
	router.use('/api/v655', require('./src/routes/v655'));
	router.use('/api/v656', require('./src/routes/v656'));
	router.use('/api/v657', require('./src/routes/v657'));
	router.use('/api/v658', require('./src/routes/v658'));
	router.use('/api/v659', require('./src/routes/v659'));
	router.use('/api/v660', require('./src/routes/v660'));
	router.use('/api/v661', require('./src/routes/v661'));
	router.use('/api/v662', require('./src/routes/v662'));
	router.use('/api/v663', require('./src/routes/v663'));
	router.use('/api/v664', require('./src/routes/v664'));
	router.use('/api/v665', require('./src/routes/v665'));
	router.use('/api/v666', require('./src/routes/v666'));
	router.use('/api/v667', require('./src/routes/v667'));
	router.use('/api/v668', require('./src/routes/v668'));
	router.use('/api/v669', require('./src/routes/v669'));
	router.use('/api/v670', require('./src/routes/v670'));
	router.use('/api/v671', require('./src/routes/v671'));
	router.use('/api/v672', require('./src/routes/v672'));
	router.use('/api/v673', require('./src/routes/v673'));
	router.use('/api/v674', require('./src/routes/v674'));
	router.use('/api/v675', require('./src/routes/v675'));
	router.use('/api/v676', require('./src/routes/v676'));
	router.use('/api/v677', require('./src/routes/v677'));
	router.use('/api/v678', require('./src/routes/v678'));
	router.use('/api/v679', require('./src/routes/v679'));
	router.use('/api/v680', require('./src/routes/v680'));
	router.use('/api/v681', require('./src/routes/v681'));
	router.use('/api/v682', require('./src/routes/v682'));
	router.use('/api/v683', require('./src/routes/v683'));
	router.use('/api/v684', require('./src/routes/v684'));
	router.use('/api/v685', require('./src/routes/v685'));
	router.use('/api/v686', require('./src/routes/v686'));
	router.use('/api/v687', require('./src/routes/v687'));
	router.use('/api/v688', require('./src/routes/v688'));
	router.use('/api/v689', require('./src/routes/v689'));
	router.use('/api/v690', require('./src/routes/v690'));
	router.use('/api/v691', require('./src/routes/v691'));
	router.use('/api/v692', require('./src/routes/v692'));
	router.use('/api/v693', require('./src/routes/v693'));
	router.use('/api/v694', require('./src/routes/v694'));
	router.use('/api/v695', require('./src/routes/v695'));
	router.use('/api/v696', require('./src/routes/v696'));
	router.use('/api/v697', require('./src/routes/v697'));
	router.use('/api/v698', require('./src/routes/v698'));
	router.use('/api/v699', require('./src/routes/v699'));
	router.use('/api/v700', require('./src/routes/v700'));
	router.use('/api/v701', require('./src/routes/v701'));
	router.use('/api/v702', require('./src/routes/v702'));
	router.use('/api/v703', require('./src/routes/v703'));
	router.use('/api/v704', require('./src/routes/v704'));
	router.use('/api/v705', require('./src/routes/v705'));
	router.use('/api/v706', require('./src/routes/v706'));
	router.use('/api/v707', require('./src/routes/v707'));
	router.use('/api/v708', require('./src/routes/v708'));
	router.use('/api/v709', require('./src/routes/v709'));
	router.use('/api/v710', require('./src/routes/v710'));
	router.use('/api/v711', require('./src/routes/v711'));
	router.use('/api/v712', require('./src/routes/v712'));
	router.use('/api/v713', require('./src/routes/v713'));
	router.use('/api/v714', require('./src/routes/v714'));
	router.use('/api/v715', require('./src/routes/v715'));
	router.use('/api/v716', require('./src/routes/v716'));
	router.use('/api/v717', require('./src/routes/v717'));
	router.use('/api/v718', require('./src/routes/v718'));
	router.use('/api/v719', require('./src/routes/v719'));
	router.use('/api/v720', require('./src/routes/v720'));
	router.use('/api/v721', require('./src/routes/v721'));
	router.use('/api/v722', require('./src/routes/v722'));
	router.use('/api/v723', require('./src/routes/v723'));
	router.use('/api/v724', require('./src/routes/v724'));
	router.use('/api/v725', require('./src/routes/v725'));
	router.use('/api/v726', require('./src/routes/v726'));
	router.use('/api/v727', require('./src/routes/v727'));
	router.use('/api/v728', require('./src/routes/v728'));
	router.use('/api/v729', require('./src/routes/v729'));
	router.use('/api/v730', require('./src/routes/v730'));
	router.use('/api/v731', require('./src/routes/v731'));
	router.use('/api/v732', require('./src/routes/v732'));
	router.use('/api/v733', require('./src/routes/v733'));
	router.use('/api/v734', require('./src/routes/v734'));
	router.use('/api/v735', require('./src/routes/v735'));
	router.use('/api/v736', require('./src/routes/v736'));
	router.use('/api/v737', require('./src/routes/v737'));
	router.use('/api/v738', require('./src/routes/v738'));
	router.use('/api/v739', require('./src/routes/v739'));
	router.use('/api/v740', require('./src/routes/v740'));
	router.use('/api/v741', require('./src/routes/v741'));
	router.use('/api/v742', require('./src/routes/v742'));
	router.use('/api/v743', require('./src/routes/v743'));
	router.use('/api/v744', require('./src/routes/v744'));
	router.use('/api/v745', require('./src/routes/v745'));
	router.use('/api/v746', require('./src/routes/v746'));
	router.use('/api/v747', require('./src/routes/v747'));
	router.use('/api/v748', require('./src/routes/v748'));
	router.use('/api/v749', require('./src/routes/v749'));
	router.use('/api/v750', require('./src/routes/v750'));
	router.use('/api/v751', require('./src/routes/v751'));
	router.use('/api/v752', require('./src/routes/v752'));
	router.use('/api/v753', require('./src/routes/v753'));
	router.use('/api/v754', require('./src/routes/v754'));
	router.use('/api/v755', require('./src/routes/v755'));
	router.use('/api/v756', require('./src/routes/v756'));
	router.use('/api/v757', require('./src/routes/v757'));
	router.use('/api/v758', require('./src/routes/v758'));
	router.use('/api/v759', require('./src/routes/v759'));
	router.use('/api/v760', require('./src/routes/v760'));
	router.use('/api/v761', require('./src/routes/v761'));
	router.use('/api/v762', require('./src/routes/v762'));
	router.use('/api/v763', require('./src/routes/v763'));
	router.use('/api/v764', require('./src/routes/v764'));
	router.use('/api/v765', require('./src/routes/v765'));
	router.use('/api/v766', require('./src/routes/v766'));
	router.use('/api/v767', require('./src/routes/v767'));
	router.use('/api/v768', require('./src/routes/v768'));
	router.use('/api/v769', require('./src/routes/v769'));
	router.use('/api/v770', require('./src/routes/v770'));
	router.use('/api/v771', require('./src/routes/v771'));
	router.use('/api/v772', require('./src/routes/v772'));
	router.use('/api/v773', require('./src/routes/v773'));
	router.use('/api/v774', require('./src/routes/v774'));
	router.use('/api/v775', require('./src/routes/v775'));
	router.use('/api/v776', require('./src/routes/v776'));
	router.use('/api/v777', require('./src/routes/v777'));
	router.use('/api/v778', require('./src/routes/v778'));
	router.use('/api/v779', require('./src/routes/v779'));
	router.use('/api/v780', require('./src/routes/v780'));
	router.use('/api/v781', require('./src/routes/v781'));
	router.use('/api/v782', require('./src/routes/v782'));
	router.use('/api/v783', require('./src/routes/v783'));
	router.use('/api/v784', require('./src/routes/v784'));
	router.use('/api/v785', require('./src/routes/v785'));
	router.use('/api/v786', require('./src/routes/v786'));
	router.use('/api/v787', require('./src/routes/v787'));
	router.use('/api/v788', require('./src/routes/v788'));
	router.use('/api/v789', require('./src/routes/v789'));
	router.use('/api/v790', require('./src/routes/v790'));
	router.use('/api/v791', require('./src/routes/v791'));
	router.use('/api/v792', require('./src/routes/v792'));
	router.use('/api/v793', require('./src/routes/v793'));
	router.use('/api/v794', require('./src/routes/v794'));
	router.use('/api/v795', require('./src/routes/v795'));
	router.use('/api/v796', require('./src/routes/v796'));
	router.use('/api/v797', require('./src/routes/v797'));
	router.use('/api/v798', require('./src/routes/v798'));
	router.use('/api/v799', require('./src/routes/v799'));
	router.use('/api/v800', require('./src/routes/v800'));
	router.use('/api/v801', require('./src/routes/v801'));
	router.use('/api/v802', require('./src/routes/v802'));
	router.use('/api/v803', require('./src/routes/v803'));
	router.use('/api/v804', require('./src/routes/v804'));
	router.use('/api/v805', require('./src/routes/v805'));
	router.use('/api/v806', require('./src/routes/v806'));
	router.use('/api/v807', require('./src/routes/v807'));
	router.use('/api/v808', require('./src/routes/v808'));
	router.use('/api/v809', require('./src/routes/v809'));
	router.use('/api/v810', require('./src/routes/v810'));
	router.use('/api/v811', require('./src/routes/v811'));
	router.use('/api/v812', require('./src/routes/v812'));
	router.use('/api/v813', require('./src/routes/v813'));
	router.use('/api/v814', require('./src/routes/v814'));
	router.use('/api/v815', require('./src/routes/v815'));
	router.use('/api/v816', require('./src/routes/v816'));
	router.use('/api/v817', require('./src/routes/v817'));
	router.use('/api/v818', require('./src/routes/v818'));
	router.use('/api/v819', require('./src/routes/v819'));
	router.use('/api/v820', require('./src/routes/v820'));
	router.use('/api/v821', require('./src/routes/v821'));
	router.use('/api/v822', require('./src/routes/v822'));
	router.use('/api/v823', require('./src/routes/v823'));
	router.use('/api/v824', require('./src/routes/v824'));
	router.use('/api/v825', require('./src/routes/v825'));
	router.use('/api/v826', require('./src/routes/v826'));
	router.use('/api/v827', require('./src/routes/v827'));
	router.use('/api/v828', require('./src/routes/v828'));
	router.use('/api/v829', require('./src/routes/v829'));
	router.use('/api/v830', require('./src/routes/v830'));
	router.use('/api/v831', require('./src/routes/v831'));
	router.use('/api/v832', require('./src/routes/v832'));
	router.use('/api/v833', require('./src/routes/v833'));
	router.use('/api/v834', require('./src/routes/v834'));
	router.use('/api/v835', require('./src/routes/v835'));
	router.use('/api/v836', require('./src/routes/v836'));
	router.use('/api/v837', require('./src/routes/v837'));
	router.use('/api/v838', require('./src/routes/v838'));
	router.use('/api/v839', require('./src/routes/v839'));
	router.use('/api/v840', require('./src/routes/v840'));
	router.use('/api/v841', require('./src/routes/v841'));
	router.use('/api/v842', require('./src/routes/v842'));
	router.use('/api/v843', require('./src/routes/v843'));
	router.use('/api/v844', require('./src/routes/v844'));
	router.use('/api/v845', require('./src/routes/v845'));
	router.use('/api/v846', require('./src/routes/v846'));
	router.use('/api/v847', require('./src/routes/v847'));
	router.use('/api/v848', require('./src/routes/v848'));
	router.use('/api/v849', require('./src/routes/v849'));
	router.use('/api/v850', require('./src/routes/v850'));
	router.use('/api/v851', require('./src/routes/v851'));
	router.use('/api/v852', require('./src/routes/v852'));
	router.use('/api/v853', require('./src/routes/v853'));
	router.use('/api/v854', require('./src/routes/v854'));
	router.use('/api/v855', require('./src/routes/v855'));
	router.use('/api/v856', require('./src/routes/v856'));
	router.use('/api/v857', require('./src/routes/v857'));
	router.use('/api/v858', require('./src/routes/v858'));
	router.use('/api/v859', require('./src/routes/v859'));
	router.use('/api/v860', require('./src/routes/v860'));
	router.use('/api/v861', require('./src/routes/v861'));
	router.use('/api/v862', require('./src/routes/v862'));
	router.use('/api/v863', require('./src/routes/v863'));
	router.use('/api/v864', require('./src/routes/v864'));
	router.use('/api/v865', require('./src/routes/v865'));
	router.use('/api/v866', require('./src/routes/v866'));
	router.use('/api/v867', require('./src/routes/v867'));
	router.use('/api/v868', require('./src/routes/v868'));
	router.use('/api/v869', require('./src/routes/v869'));
	router.use('/api/v870', require('./src/routes/v870'));
	router.use('/api/v871', require('./src/routes/v871'));
	router.use('/api/v872', require('./src/routes/v872'));
	router.use('/api/v873', require('./src/routes/v873'));
	router.use('/api/v874', require('./src/routes/v874'));
	router.use('/api/v875', require('./src/routes/v875'));
	router.use('/api/v876', require('./src/routes/v876'));
	router.use('/api/v877', require('./src/routes/v877'));
	router.use('/api/v878', require('./src/routes/v878'));
	router.use('/api/v879', require('./src/routes/v879'));
	router.use('/api/v880', require('./src/routes/v880'));
	router.use('/api/v881', require('./src/routes/v881'));
	router.use('/api/v882', require('./src/routes/v882'));
	router.use('/api/v883', require('./src/routes/v883'));
	router.use('/api/v884', require('./src/routes/v884'));
	router.use('/api/v885', require('./src/routes/v885'));
	router.use('/api/v886', require('./src/routes/v886'));
	router.use('/api/v887', require('./src/routes/v887'));
	router.use('/api/v888', require('./src/routes/v888'));
	router.use('/api/v889', require('./src/routes/v889'));
	router.use('/api/v890', require('./src/routes/v890'));
	router.use('/api/v891', require('./src/routes/v891'));
	router.use('/api/v892', require('./src/routes/v892'));
	router.use('/api/v893', require('./src/routes/v893'));
	router.use('/api/v894', require('./src/routes/v894'));
	router.use('/api/v895', require('./src/routes/v895'));
	router.use('/api/v896', require('./src/routes/v896'));
	router.use('/api/v897', require('./src/routes/v897'));
	router.use('/api/v898', require('./src/routes/v898'));
	router.use('/api/v899', require('./src/routes/v899'));
	router.use('/api/v900', require('./src/routes/v900'));
	router.use('/api/v901', require('./src/routes/v901'));
	router.use('/api/v902', require('./src/routes/v902'));
	router.use('/api/v903', require('./src/routes/v903'));
	router.use('/api/v904', require('./src/routes/v904'));
	router.use('/api/v905', require('./src/routes/v905'));
	router.use('/api/v906', require('./src/routes/v906'));
	router.use('/api/v907', require('./src/routes/v907'));
	router.use('/api/v908', require('./src/routes/v908'));
	router.use('/api/v909', require('./src/routes/v909'));
	router.use('/api/v910', require('./src/routes/v910'));
	router.use('/api/v911', require('./src/routes/v911'));
	router.use('/api/v912', require('./src/routes/v912'));
	router.use('/api/v913', require('./src/routes/v913'));
	router.use('/api/v914', require('./src/routes/v914'));
	router.use('/api/v915', require('./src/routes/v915'));
	router.use('/api/v916', require('./src/routes/v916'));
	router.use('/api/v917', require('./src/routes/v917'));
	router.use('/api/v918', require('./src/routes/v918'));
	router.use('/api/v919', require('./src/routes/v919'));
	router.use('/api/v920', require('./src/routes/v920'));
	router.use('/api/v921', require('./src/routes/v921'));
	router.use('/api/v922', require('./src/routes/v922'));
	router.use('/api/v923', require('./src/routes/v923'));
	router.use('/api/v924', require('./src/routes/v924'));
	router.use('/api/v925', require('./src/routes/v925'));
	router.use('/api/v926', require('./src/routes/v926'));
	router.use('/api/v927', require('./src/routes/v927'));
	router.use('/api/v928', require('./src/routes/v928'));
	router.use('/api/v929', require('./src/routes/v929'));
	router.use('/api/v930', require('./src/routes/v930'));
	router.use('/api/v931', require('./src/routes/v931'));
	router.use('/api/v932', require('./src/routes/v932'));
	router.use('/api/v933', require('./src/routes/v933'));
	router.use('/api/v934', require('./src/routes/v934'));
	router.use('/api/v935', require('./src/routes/v935'));
	router.use('/api/v936', require('./src/routes/v936'));
	router.use('/api/v937', require('./src/routes/v937'));
	router.use('/api/v938', require('./src/routes/v938'));
	router.use('/api/v939', require('./src/routes/v939'));
	router.use('/api/v940', require('./src/routes/v940'));
	router.use('/api/v941', require('./src/routes/v941'));
	router.use('/api/v942', require('./src/routes/v942'));
	router.use('/api/v943', require('./src/routes/v943'));
	router.use('/api/v944', require('./src/routes/v944'));
	router.use('/api/v945', require('./src/routes/v945'));
	router.use('/api/v946', require('./src/routes/v946'));
	router.use('/api/v947', require('./src/routes/v947'));
	router.use('/api/v948', require('./src/routes/v948'));
	router.use('/api/v949', require('./src/routes/v949'));
	router.use('/api/v950', require('./src/routes/v950'));
	router.use('/api/v951', require('./src/routes/v951'));
	router.use('/api/v952', require('./src/routes/v952'));
	router.use('/api/v953', require('./src/routes/v953'));
	router.use('/api/v954', require('./src/routes/v954'));
	router.use('/api/v955', require('./src/routes/v955'));
	router.use('/api/v956', require('./src/routes/v956'));
	router.use('/api/v957', require('./src/routes/v957'));
	router.use('/api/v958', require('./src/routes/v958'));
	router.use('/api/v959', require('./src/routes/v959'));
	router.use('/api/v960', require('./src/routes/v960'));
	router.use('/api/v961', require('./src/routes/v961'));
	router.use('/api/v962', require('./src/routes/v962'));
	router.use('/api/v963', require('./src/routes/v963'));
	router.use('/api/v964', require('./src/routes/v964'));
	router.use('/api/v965', require('./src/routes/v965'));
	router.use('/api/v966', require('./src/routes/v966'));
	router.use('/api/v967', require('./src/routes/v967'));
	router.use('/api/v968', require('./src/routes/v968'));
	router.use('/api/v969', require('./src/routes/v969'));
	router.use('/api/v970', require('./src/routes/v970'));
	router.use('/api/v971', require('./src/routes/v971'));
	router.use('/api/v972', require('./src/routes/v972'));
	router.use('/api/v973', require('./src/routes/v973'));
	router.use('/api/v974', require('./src/routes/v974'));
	router.use('/api/v975', require('./src/routes/v975'));
	router.use('/api/v976', require('./src/routes/v976'));
	router.use('/api/v977', require('./src/routes/v977'));
	router.use('/api/v978', require('./src/routes/v978'));
	router.use('/api/v979', require('./src/routes/v979'));
	router.use('/api/v980', require('./src/routes/v980'));
	router.use('/api/v981', require('./src/routes/v981'));
	router.use('/api/v982', require('./src/routes/v982'));
	router.use('/api/v983', require('./src/routes/v983'));
	router.use('/api/v984', require('./src/routes/v984'));
	router.use('/api/v985', require('./src/routes/v985'));
	router.use('/api/v986', require('./src/routes/v986'));
	router.use('/api/v987', require('./src/routes/v987'));
	router.use('/api/v988', require('./src/routes/v988'));
	router.use('/api/v989', require('./src/routes/v989'));
	router.use('/api/v990', require('./src/routes/v990'));
	router.use('/api/v991', require('./src/routes/v991'));
	router.use('/api/v992', require('./src/routes/v992'));
	router.use('/api/v993', require('./src/routes/v993'));
	router.use('/api/v994', require('./src/routes/v994'));
	router.use('/api/v995', require('./src/routes/v995'));
	router.use('/api/v996', require('./src/routes/v996'));
	router.use('/api/v997', require('./src/routes/v997'));
	router.use('/api/v998', require('./src/routes/v998'));
	router.use('/api/v999', require('./src/routes/v999'));
	router.use('/api/v1000', require('./src/routes/v1000'));
	router.use('/api/v1001', require('./src/routes/v1001'));
	router.use('/api/v1002', require('./src/routes/v1002'));
	router.use('/api/v1003', require('./src/routes/v1003'));
	router.use('/api/v1004', require('./src/routes/v1004'));
	router.use('/api/v1005', require('./src/routes/v1005'));
	router.use('/api/v1006', require('./src/routes/v1006'));
	router.use('/api/v1007', require('./src/routes/v1007'));
	router.use('/api/v1008', require('./src/routes/v1008'));
	router.use('/api/v1009', require('./src/routes/v1009'));
	router.use('/api/v1010', require('./src/routes/v1010'));
	router.use('/api/v1011', require('./src/routes/v1011'));
	router.use('/api/v1012', require('./src/routes/v1012'));
	router.use('/api/v1013', require('./src/routes/v1013'));
	router.use('/api/v1014', require('./src/routes/v1014'));
	router.use('/api/v1015', require('./src/routes/v1015'));
	router.use('/api/v1016', require('./src/routes/v1016'));
	router.use('/api/v1017', require('./src/routes/v1017'));
	router.use('/api/v1018', require('./src/routes/v1018'));
	router.use('/api/v1019', require('./src/routes/v1019'));
	router.use('/api/v1020', require('./src/routes/v1020'));
	router.use('/api/v1021', require('./src/routes/v1021'));
	router.use('/api/v1022', require('./src/routes/v1022'));
	router.use('/api/v1023', require('./src/routes/v1023'));
	router.use('/api/v1024', require('./src/routes/v1024'));
	router.use('/api/v1025', require('./src/routes/v1025'));
	router.use('/api/v1026', require('./src/routes/v1026'));
	router.use('/api/v1027', require('./src/routes/v1027'));
	router.use('/api/v1028', require('./src/routes/v1028'));
	router.use('/api/v1029', require('./src/routes/v1029'));
	router.use('/api/v1030', require('./src/routes/v1030'));
	router.use('/api/v1031', require('./src/routes/v1031'));
	router.use('/api/v1032', require('./src/routes/v1032'));
	router.use('/api/v1033', require('./src/routes/v1033'));
	router.use('/api/v1034', require('./src/routes/v1034'));
	router.use('/api/v1035', require('./src/routes/v1035'));
	router.use('/api/v1036', require('./src/routes/v1036'));
	router.use('/api/v1037', require('./src/routes/v1037'));
	router.use('/api/v1038', require('./src/routes/v1038'));
	router.use('/api/v1039', require('./src/routes/v1039'));
	router.use('/api/v1040', require('./src/routes/v1040'));
	router.use('/api/v1041', require('./src/routes/v1041'));
	router.use('/api/v1042', require('./src/routes/v1042'));
	router.use('/api/v1043', require('./src/routes/v1043'));
	router.use('/api/v1044', require('./src/routes/v1044'));
	router.use('/api/v1045', require('./src/routes/v1045'));
	router.use('/api/v1046', require('./src/routes/v1046'));
	router.use('/api/v1047', require('./src/routes/v1047'));
	router.use('/api/v1048', require('./src/routes/v1048'));
	router.use('/api/v1049', require('./src/routes/v1049'));
	router.use('/api/v1050', require('./src/routes/v1050'));
	router.use('/api/v1051', require('./src/routes/v1051'));
	router.use('/api/v1052', require('./src/routes/v1052'));
	router.use('/api/v1053', require('./src/routes/v1053'));
	router.use('/api/v1054', require('./src/routes/v1054'));
	router.use('/api/v1055', require('./src/routes/v1055'));
	router.use('/api/v1056', require('./src/routes/v1056'));
	router.use('/api/v1057', require('./src/routes/v1057'));
	router.use('/api/v1058', require('./src/routes/v1058'));
	router.use('/api/v1059', require('./src/routes/v1059'));
	router.use('/api/v1060', require('./src/routes/v1060'));
	router.use('/api/v1061', require('./src/routes/v1061'));
	router.use('/api/v1062', require('./src/routes/v1062'));
	router.use('/api/v1063', require('./src/routes/v1063'));
	router.use('/api/v1064', require('./src/routes/v1064'));
	router.use('/api/v1065', require('./src/routes/v1065'));
	router.use('/api/v1066', require('./src/routes/v1066'));
	router.use('/api/v1067', require('./src/routes/v1067'));
	router.use('/api/v1068', require('./src/routes/v1068'));
	router.use('/api/v1069', require('./src/routes/v1069'));
	router.use('/api/v1070', require('./src/routes/v1070'));
	router.use('/api/v1071', require('./src/routes/v1071'));
	router.use('/api/v1072', require('./src/routes/v1072'));
	router.use('/api/v1073', require('./src/routes/v1073'));
	router.use('/api/v1074', require('./src/routes/v1074'));
	router.use('/api/v1075', require('./src/routes/v1075'));
	router.use('/api/v1076', require('./src/routes/v1076'));
	router.use('/api/v1077', require('./src/routes/v1077'));
	router.use('/api/v1078', require('./src/routes/v1078'));
	router.use('/api/v1079', require('./src/routes/v1079'));
	router.use('/api/v1080', require('./src/routes/v1080'));
	router.use('/api/v1081', require('./src/routes/v1081'));
	router.use('/api/v1082', require('./src/routes/v1082'));
	router.use('/api/v1083', require('./src/routes/v1083'));
	router.use('/api/v1084', require('./src/routes/v1084'));
	router.use('/api/v1085', require('./src/routes/v1085'));
	router.use('/api/v1086', require('./src/routes/v1086'));
	router.use('/api/v1087', require('./src/routes/v1087'));
	router.use('/api/v1088', require('./src/routes/v1088'));
	router.use('/api/v1089', require('./src/routes/v1089'));
	router.use('/api/v1090', require('./src/routes/v1090'));
	router.use('/api/v1091', require('./src/routes/v1091'));
	router.use('/api/v1092', require('./src/routes/v1092'));
	router.use('/api/v1093', require('./src/routes/v1093'));
	router.use('/api/v1094', require('./src/routes/v1094'));
	router.use('/api/v1095', require('./src/routes/v1095'));
	router.use('/api/v1096', require('./src/routes/v1096'));
	router.use('/api/v1097', require('./src/routes/v1097'));
	router.use('/api/v1098', require('./src/routes/v1098'));
	router.use('/api/v1099', require('./src/routes/v1099'));
	router.use('/api/v1100', require('./src/routes/v1100'));
	router.use('/api/v1101', require('./src/routes/v1101'));
	router.use('/api/v1102', require('./src/routes/v1102'));
	router.use('/api/v1103', require('./src/routes/v1103'));
	router.use('/api/v1104', require('./src/routes/v1104'));
	router.use('/api/v1105', require('./src/routes/v1105'));
	router.use('/api/v1106', require('./src/routes/v1106'));
	router.use('/api/v1107', require('./src/routes/v1107'));
	router.use('/api/v1108', require('./src/routes/v1108'));
	router.use('/api/v1109', require('./src/routes/v1109'));
	router.use('/api/v1110', require('./src/routes/v1110'));
	router.use('/api/v1111', require('./src/routes/v1111'));
	router.use('/api/v1112', require('./src/routes/v1112'));
	router.use('/api/v1113', require('./src/routes/v1113'));
	router.use('/api/v1114', require('./src/routes/v1114'));
	router.use('/api/v1115', require('./src/routes/v1115'));
	router.use('/api/v1116', require('./src/routes/v1116'));
	router.use('/api/v1117', require('./src/routes/v1117'));
	router.use('/api/v1118', require('./src/routes/v1118'));
	router.use('/api/v1119', require('./src/routes/v1119'));
	router.use('/api/v1120', require('./src/routes/v1120'));
	router.use('/api/v1121', require('./src/routes/v1121'));
	router.use('/api/v1122', require('./src/routes/v1122'));
	router.use('/api/v1123', require('./src/routes/v1123'));
	router.use('/api/v1124', require('./src/routes/v1124'));
	router.use('/api/v1125', require('./src/routes/v1125'));
	router.use('/api/v1126', require('./src/routes/v1126'));
	router.use('/api/v1127', require('./src/routes/v1127'));
	router.use('/api/v1128', require('./src/routes/v1128'));
	router.use('/api/v1129', require('./src/routes/v1129'));
	router.use('/api/v1130', require('./src/routes/v1130'));
	router.use('/api/v1131', require('./src/routes/v1131'));
	router.use('/api/v1132', require('./src/routes/v1132'));
	router.use('/api/v1133', require('./src/routes/v1133'));
	router.use('/api/v1134', require('./src/routes/v1134'));
	router.use('/api/v1135', require('./src/routes/v1135'));
	router.use('/api/v1136', require('./src/routes/v1136'));
	router.use('/api/v1137', require('./src/routes/v1137'));
	router.use('/api/v1138', require('./src/routes/v1138'));
	router.use('/api/v1139', require('./src/routes/v1139'));
	router.use('/api/v1140', require('./src/routes/v1140'));
	router.use('/api/v1141', require('./src/routes/v1141'));
	router.use('/api/v1142', require('./src/routes/v1142'));
	router.use('/api/v1143', require('./src/routes/v1143'));
	router.use('/api/v1144', require('./src/routes/v1144'));
	router.use('/api/v1145', require('./src/routes/v1145'));
	router.use('/api/v1146', require('./src/routes/v1146'));
	router.use('/api/v1147', require('./src/routes/v1147'));
	router.use('/api/v1148', require('./src/routes/v1148'));
	router.use('/api/v1149', require('./src/routes/v1149'));
	router.use('/api/v1150', require('./src/routes/v1150'));
	router.use('/api/v1151', require('./src/routes/v1151'));
	router.use('/api/v1152', require('./src/routes/v1152'));
	router.use('/api/v1153', require('./src/routes/v1153'));
	router.use('/api/v1154', require('./src/routes/v1154'));
	router.use('/api/v1155', require('./src/routes/v1155'));
	router.use('/api/v1156', require('./src/routes/v1156'));
	router.use('/api/v1157', require('./src/routes/v1157'));
	router.use('/api/v1158', require('./src/routes/v1158'));
	router.use('/api/v1159', require('./src/routes/v1159'));
	router.use('/api/v1160', require('./src/routes/v1160'));
	router.use('/api/v1161', require('./src/routes/v1161'));
	router.use('/api/v1162', require('./src/routes/v1162'));
	router.use('/api/v1163', require('./src/routes/v1163'));
	router.use('/api/v1164', require('./src/routes/v1164'));
	router.use('/api/v1165', require('./src/routes/v1165'));
	router.use('/api/v1166', require('./src/routes/v1166'));
	router.use('/api/v1167', require('./src/routes/v1167'));
	router.use('/api/v1168', require('./src/routes/v1168'));
	router.use('/api/v1169', require('./src/routes/v1169'));
	router.use('/api/v1170', require('./src/routes/v1170'));
	router.use('/api/v1171', require('./src/routes/v1171'));
	router.use('/api/v1172', require('./src/routes/v1172'));
	router.use('/api/v1173', require('./src/routes/v1173'));
	router.use('/api/v1174', require('./src/routes/v1174'));
	router.use('/api/v1175', require('./src/routes/v1175'));
	router.use('/api/v1176', require('./src/routes/v1176'));
	router.use('/api/v1177', require('./src/routes/v1177'));
	router.use('/api/v1178', require('./src/routes/v1178'));
	router.use('/api/v1179', require('./src/routes/v1179'));
	router.use('/api/v1180', require('./src/routes/v1180'));
	router.use('/api/v1181', require('./src/routes/v1181'));
	router.use('/api/v1182', require('./src/routes/v1182'));
	router.use('/api/v1183', require('./src/routes/v1183'));
	router.use('/api/v1184', require('./src/routes/v1184'));
	router.use('/api/v1185', require('./src/routes/v1185'));
	router.use('/api/v1186', require('./src/routes/v1186'));
	router.use('/api/v1187', require('./src/routes/v1187'));
	router.use('/api/v1188', require('./src/routes/v1188'));
	router.use('/api/v1189', require('./src/routes/v1189'));
	router.use('/api/v1190', require('./src/routes/v1190'));
	router.use('/api/v1191', require('./src/routes/v1191'));
	router.use('/api/v1192', require('./src/routes/v1192'));
	router.use('/api/v1193', require('./src/routes/v1193'));
	router.use('/api/v1194', require('./src/routes/v1194'));
	router.use('/api/v1195', require('./src/routes/v1195'));
	router.use('/api/v1196', require('./src/routes/v1196'));
	router.use('/api/v1197', require('./src/routes/v1197'));
	router.use('/api/v1198', require('./src/routes/v1198'));
	router.use('/api/v1199', require('./src/routes/v1199'));
	router.use('/api/v1200', require('./src/routes/v1200'));
	router.use('/api/v1201', require('./src/routes/v1201'));
	router.use('/api/v1202', require('./src/routes/v1202'));
	router.use('/api/v1203', require('./src/routes/v1203'));
	router.use('/api/v1204', require('./src/routes/v1204'));
	router.use('/api/v1205', require('./src/routes/v1205'));
	router.use('/api/v1206', require('./src/routes/v1206'));
	router.use('/api/v1207', require('./src/routes/v1207'));
	router.use('/api/v1208', require('./src/routes/v1208'));
	router.use('/api/v1209', require('./src/routes/v1209'));
	router.use('/api/v1210', require('./src/routes/v1210'));
	router.use('/api/v1211', require('./src/routes/v1211'));
	router.use('/api/v1212', require('./src/routes/v1212'));
	router.use('/api/v1213', require('./src/routes/v1213'));
	router.use('/api/v1214', require('./src/routes/v1214'));
	router.use('/api/v1215', require('./src/routes/v1215'));
	router.use('/api/v1216', require('./src/routes/v1216'));
	router.use('/api/v1217', require('./src/routes/v1217'));
	router.use('/api/v1218', require('./src/routes/v1218'));
	router.use('/api/v1219', require('./src/routes/v1219'));
	router.use('/api/v1220', require('./src/routes/v1220'));
	router.use('/api/v1221', require('./src/routes/v1221'));
	router.use('/api/v1222', require('./src/routes/v1222'));
	router.use('/api/v1223', require('./src/routes/v1223'));
	router.use('/api/v1224', require('./src/routes/v1224'));
	router.use('/api/v1225', require('./src/routes/v1225'));
	router.use('/api/v1226', require('./src/routes/v1226'));
	router.use('/api/v1227', require('./src/routes/v1227'));
	router.use('/api/v1228', require('./src/routes/v1228'));
	router.use('/api/v1229', require('./src/routes/v1229'));
	router.use('/api/v1230', require('./src/routes/v1230'));
	router.use('/api/v1231', require('./src/routes/v1231'));
	router.use('/api/v1232', require('./src/routes/v1232'));
	router.use('/api/v1233', require('./src/routes/v1233'));
	router.use('/api/v1234', require('./src/routes/v1234'));
	router.use('/api/v1235', require('./src/routes/v1235'));
	router.use('/api/v1236', require('./src/routes/v1236'));
	router.use('/api/v1237', require('./src/routes/v1237'));
	router.use('/api/v1238', require('./src/routes/v1238'));
	router.use('/api/v1239', require('./src/routes/v1239'));
	router.use('/api/v1240', require('./src/routes/v1240'));
	router.use('/api/v1241', require('./src/routes/v1241'));
	router.use('/api/v1242', require('./src/routes/v1242'));
	router.use('/api/v1243', require('./src/routes/v1243'));
	router.use('/api/v1244', require('./src/routes/v1244'));
	router.use('/api/v1245', require('./src/routes/v1245'));
	router.use('/api/v1246', require('./src/routes/v1246'));
	router.use('/api/v1247', require('./src/routes/v1247'));
	router.use('/api/v1248', require('./src/routes/v1248'));
	router.use('/api/v1249', require('./src/routes/v1249'));
	router.use('/api/v1250', require('./src/routes/v1250'));
	router.use('/api/v1251', require('./src/routes/v1251'));
	router.use('/api/v1252', require('./src/routes/v1252'));
	router.use('/api/v1253', require('./src/routes/v1253'));
	router.use('/api/v1254', require('./src/routes/v1254'));
	router.use('/api/v1255', require('./src/routes/v1255'));
	router.use('/api/v1256', require('./src/routes/v1256'));
	router.use('/api/v1257', require('./src/routes/v1257'));
	router.use('/api/v1258', require('./src/routes/v1258'));
	router.use('/api/v1259', require('./src/routes/v1259'));
	router.use('/api/v1260', require('./src/routes/v1260'));
	router.use('/api/v1261', require('./src/routes/v1261'));
	router.use('/api/v1262', require('./src/routes/v1262'));
	router.use('/api/v1263', require('./src/routes/v1263'));
	router.use('/api/v1264', require('./src/routes/v1264'));
	router.use('/api/v1265', require('./src/routes/v1265'));
	router.use('/api/v1266', require('./src/routes/v1266'));
	router.use('/api/v1267', require('./src/routes/v1267'));
	router.use('/api/v1268', require('./src/routes/v1268'));
	router.use('/api/v1269', require('./src/routes/v1269'));
	router.use('/api/v1270', require('./src/routes/v1270'));
	router.use('/api/v1271', require('./src/routes/v1271'));
	router.use('/api/v1272', require('./src/routes/v1272'));
	router.use('/api/v1273', require('./src/routes/v1273'));
	router.use('/api/v1274', require('./src/routes/v1274'));
	router.use('/api/v1275', require('./src/routes/v1275'));
	router.use('/api/v1276', require('./src/routes/v1276'));
	router.use('/api/v1277', require('./src/routes/v1277'));
	router.use('/api/v1278', require('./src/routes/v1278'));
	router.use('/api/v1279', require('./src/routes/v1279'));
	router.use('/api/v1280', require('./src/routes/v1280'));
	router.use('/api/v1281', require('./src/routes/v1281'));
	router.use('/api/v1282', require('./src/routes/v1282'));
	router.use('/api/v1283', require('./src/routes/v1283'));
	router.use('/api/v1284', require('./src/routes/v1284'));
	router.use('/api/v1285', require('./src/routes/v1285'));
	router.use('/api/v1286', require('./src/routes/v1286'));
	router.use('/api/v1287', require('./src/routes/v1287'));
	router.use('/api/v1288', require('./src/routes/v1288'));
	router.use('/api/v1289', require('./src/routes/v1289'));
	router.use('/api/v1290', require('./src/routes/v1290'));
	router.use('/api/v1291', require('./src/routes/v1291'));
	router.use('/api/v1292', require('./src/routes/v1292'));
	router.use('/api/v1293', require('./src/routes/v1293'));
	router.use('/api/v1294', require('./src/routes/v1294'));
	router.use('/api/v1295', require('./src/routes/v1295'));
	router.use('/api/v1296', require('./src/routes/v1296'));
	router.use('/api/v1297', require('./src/routes/v1297'));
	router.use('/api/v1298', require('./src/routes/v1298'));
	router.use('/api/v1299', require('./src/routes/v1299'));
	router.use('/api/v1300', require('./src/routes/v1300'));
	router.use('/api/v1301', require('./src/routes/v1301'));
	router.use('/api/v1302', require('./src/routes/v1302'));
	router.use('/api/v1303', require('./src/routes/v1303'));
	router.use('/api/v1304', require('./src/routes/v1304'));
	router.use('/api/v1305', require('./src/routes/v1305'));
	router.use('/api/v1306', require('./src/routes/v1306'));
	router.use('/api/v1307', require('./src/routes/v1307'));
	router.use('/api/v1308', require('./src/routes/v1308'));
	router.use('/api/v1309', require('./src/routes/v1309'));
	router.use('/api/v1310', require('./src/routes/v1310'));
	router.use('/api/v1311', require('./src/routes/v1311'));
	router.use('/api/v1312', require('./src/routes/v1312'));
	router.use('/api/v1313', require('./src/routes/v1313'));
	router.use('/api/v1314', require('./src/routes/v1314'));
	router.use('/api/v1315', require('./src/routes/v1315'));
	router.use('/api/v1316', require('./src/routes/v1316'));
	router.use('/api/v1317', require('./src/routes/v1317'));
	router.use('/api/v1318', require('./src/routes/v1318'));
	router.use('/api/v1319', require('./src/routes/v1319'));
	router.use('/api/v1320', require('./src/routes/v1320'));
	router.use('/api/v1321', require('./src/routes/v1321'));
	router.use('/api/v1322', require('./src/routes/v1322'));
	router.use('/api/v1323', require('./src/routes/v1323'));
	router.use('/api/v1324', require('./src/routes/v1324'));
	router.use('/api/v1325', require('./src/routes/v1325'));
	router.use('/api/v1326', require('./src/routes/v1326'));
	router.use('/api/v1327', require('./src/routes/v1327'));
	router.use('/api/v1328', require('./src/routes/v1328'));
	router.use('/api/v1329', require('./src/routes/v1329'));
	router.use('/api/v1330', require('./src/routes/v1330'));
	router.use('/api/v1331', require('./src/routes/v1331'));
	router.use('/api/v1332', require('./src/routes/v1332'));
	router.use('/api/v1333', require('./src/routes/v1333'));
	router.use('/api/v1334', require('./src/routes/v1334'));
	router.use('/api/v1335', require('./src/routes/v1335'));
	router.use('/api/v1336', require('./src/routes/v1336'));
	router.use('/api/v1337', require('./src/routes/v1337'));
	router.use('/api/v1338', require('./src/routes/v1338'));
	router.use('/api/v1339', require('./src/routes/v1339'));
	router.use('/api/v1340', require('./src/routes/v1340'));
	router.use('/api/v1341', require('./src/routes/v1341'));
	router.use('/api/v1342', require('./src/routes/v1342'));
	router.use('/api/v1343', require('./src/routes/v1343'));
	router.use('/api/v1344', require('./src/routes/v1344'));
	router.use('/api/v1345', require('./src/routes/v1345'));
	router.use('/api/v1346', require('./src/routes/v1346'));
	router.use('/api/v1347', require('./src/routes/v1347'));
	router.use('/api/v1348', require('./src/routes/v1348'));
	router.use('/api/v1349', require('./src/routes/v1349'));
	router.use('/api/v1350', require('./src/routes/v1350'));
	router.use('/api/v1351', require('./src/routes/v1351'));
	router.use('/api/v1352', require('./src/routes/v1352'));
	router.use('/api/v1353', require('./src/routes/v1353'));
	router.use('/api/v1354', require('./src/routes/v1354'));
	router.use('/api/v1355', require('./src/routes/v1355'));
	router.use('/api/v1356', require('./src/routes/v1356'));
	router.use('/api/v1357', require('./src/routes/v1357'));
	router.use('/api/v1358', require('./src/routes/v1358'));
	router.use('/api/v1359', require('./src/routes/v1359'));
	router.use('/api/v1360', require('./src/routes/v1360'));
	router.use('/api/v1361', require('./src/routes/v1361'));
	router.use('/api/v1362', require('./src/routes/v1362'));
	router.use('/api/v1363', require('./src/routes/v1363'));
	router.use('/api/v1364', require('./src/routes/v1364'));
	router.use('/api/v1365', require('./src/routes/v1365'));
	router.use('/api/v1366', require('./src/routes/v1366'));
	router.use('/api/v1367', require('./src/routes/v1367'));
	router.use('/api/v1368', require('./src/routes/v1368'));
	router.use('/api/v1369', require('./src/routes/v1369'));
	router.use('/api/v1370', require('./src/routes/v1370'));
	router.use('/api/v1371', require('./src/routes/v1371'));
	router.use('/api/v1372', require('./src/routes/v1372'));
	router.use('/api/v1373', require('./src/routes/v1373'));
	router.use('/api/v1374', require('./src/routes/v1374'));
	router.use('/api/v1375', require('./src/routes/v1375'));
	router.use('/api/v1376', require('./src/routes/v1376'));
	router.use('/api/v1377', require('./src/routes/v1377'));
	router.use('/api/v1378', require('./src/routes/v1378'));
	router.use('/api/v1379', require('./src/routes/v1379'));
	router.use('/api/v1380', require('./src/routes/v1380'));
	router.use('/api/v1381', require('./src/routes/v1381'));
	router.use('/api/v1382', require('./src/routes/v1382'));
	router.use('/api/v1383', require('./src/routes/v1383'));
	router.use('/api/v1384', require('./src/routes/v1384'));
	router.use('/api/v1385', require('./src/routes/v1385'));
	router.use('/api/v1386', require('./src/routes/v1386'));
	router.use('/api/v1387', require('./src/routes/v1387'));
	router.use('/api/v1388', require('./src/routes/v1388'));
	router.use('/api/v1389', require('./src/routes/v1389'));
	router.use('/api/v1390', require('./src/routes/v1390'));
	router.use('/api/v1391', require('./src/routes/v1391'));
	router.use('/api/v1392', require('./src/routes/v1392'));
	router.use('/api/v1393', require('./src/routes/v1393'));
	router.use('/api/v1394', require('./src/routes/v1394'));
	router.use('/api/v1395', require('./src/routes/v1395'));
	router.use('/api/v1396', require('./src/routes/v1396'));
	router.use('/api/v1397', require('./src/routes/v1397'));
	router.use('/api/v1398', require('./src/routes/v1398'));
	router.use('/api/v1399', require('./src/routes/v1399'));
	router.use('/api/v1400', require('./src/routes/v1400'));
	router.use('/api/v1401', require('./src/routes/v1401'));
	router.use('/api/v1402', require('./src/routes/v1402'));
	router.use('/api/v1403', require('./src/routes/v1403'));
	router.use('/api/v1404', require('./src/routes/v1404'));
	router.use('/api/v1405', require('./src/routes/v1405'));
	router.use('/api/v1406', require('./src/routes/v1406'));
	router.use('/api/v1407', require('./src/routes/v1407'));
	router.use('/api/v1408', require('./src/routes/v1408'));
	router.use('/api/v1409', require('./src/routes/v1409'));
	router.use('/api/v1410', require('./src/routes/v1410'));
	router.use('/api/v1411', require('./src/routes/v1411'));
	router.use('/api/v1412', require('./src/routes/v1412'));
	router.use('/api/v1413', require('./src/routes/v1413'));
	router.use('/api/v1414', require('./src/routes/v1414'));
	router.use('/api/v1415', require('./src/routes/v1415'));
	router.use('/api/v1416', require('./src/routes/v1416'));
	router.use('/api/v1417', require('./src/routes/v1417'));
	router.use('/api/v1418', require('./src/routes/v1418'));
	router.use('/api/v1419', require('./src/routes/v1419'));
	router.use('/api/v1420', require('./src/routes/v1420'));
	router.use('/api/v1421', require('./src/routes/v1421'));
	router.use('/api/v1422', require('./src/routes/v1422'));
	router.use('/api/v1423', require('./src/routes/v1423'));
	router.use('/api/v1424', require('./src/routes/v1424'));
	router.use('/api/v1425', require('./src/routes/v1425'));
	router.use('/api/v1426', require('./src/routes/v1426'));
	router.use('/api/v1427', require('./src/routes/v1427'));
	router.use('/api/v1428', require('./src/routes/v1428'));
	router.use('/api/v1429', require('./src/routes/v1429'));
	router.use('/api/v1430', require('./src/routes/v1430'));
	router.use('/api/v1431', require('./src/routes/v1431'));
	router.use('/api/v1432', require('./src/routes/v1432'));
	router.use('/api/v1433', require('./src/routes/v1433'));
	router.use('/api/v1434', require('./src/routes/v1434'));
	router.use('/api/v1435', require('./src/routes/v1435'));
	router.use('/api/v1436', require('./src/routes/v1436'));
	router.use('/api/v1437', require('./src/routes/v1437'));
	router.use('/api/v1438', require('./src/routes/v1438'));
	router.use('/api/v1439', require('./src/routes/v1439'));
	router.use('/api/v1440', require('./src/routes/v1440'));
	router.use('/api/v1441', require('./src/routes/v1441'));
	router.use('/api/v1442', require('./src/routes/v1442'));
	router.use('/api/v1443', require('./src/routes/v1443'));
	router.use('/api/v1444', require('./src/routes/v1444'));
	router.use('/api/v1445', require('./src/routes/v1445'));
	router.use('/api/v1446', require('./src/routes/v1446'));
	router.use('/api/v1447', require('./src/routes/v1447'));
	router.use('/api/v1448', require('./src/routes/v1448'));
	router.use('/api/v1449', require('./src/routes/v1449'));
	router.use('/api/v1450', require('./src/routes/v1450'));
	router.use('/api/v1451', require('./src/routes/v1451'));
	router.use('/api/v1452', require('./src/routes/v1452'));
	router.use('/api/v1453', require('./src/routes/v1453'));
	router.use('/api/v1454', require('./src/routes/v1454'));
	router.use('/api/v1455', require('./src/routes/v1455'));
	router.use('/api/v1456', require('./src/routes/v1456'));
	router.use('/api/v1457', require('./src/routes/v1457'));
	router.use('/api/v1458', require('./src/routes/v1458'));
	router.use('/api/v1459', require('./src/routes/v1459'));
	router.use('/api/v1460', require('./src/routes/v1460'));
	router.use('/api/v1461', require('./src/routes/v1461'));
	router.use('/api/v1462', require('./src/routes/v1462'));
	router.use('/api/v1463', require('./src/routes/v1463'));
	router.use('/api/v1464', require('./src/routes/v1464'));
	router.use('/api/v1465', require('./src/routes/v1465'));
	router.use('/api/v1466', require('./src/routes/v1466'));
	router.use('/api/v1467', require('./src/routes/v1467'));
	router.use('/api/v1468', require('./src/routes/v1468'));
	router.use('/api/v1469', require('./src/routes/v1469'));
	router.use('/api/v1470', require('./src/routes/v1470'));
	router.use('/api/v1471', require('./src/routes/v1471'));
	router.use('/api/v1472', require('./src/routes/v1472'));
	router.use('/api/v1473', require('./src/routes/v1473'));
	router.use('/api/v1474', require('./src/routes/v1474'));
	router.use('/api/v1475', require('./src/routes/v1475'));
	router.use('/api/v1476', require('./src/routes/v1476'));
	router.use('/api/v1477', require('./src/routes/v1477'));
	router.use('/api/v1478', require('./src/routes/v1478'));
	router.use('/api/v1479', require('./src/routes/v1479'));
	router.use('/api/v1480', require('./src/routes/v1480'));
	router.use('/api/v1481', require('./src/routes/v1481'));
	router.use('/api/v1482', require('./src/routes/v1482'));
	router.use('/api/v1483', require('./src/routes/v1483'));
	router.use('/api/v1484', require('./src/routes/v1484'));
	router.use('/api/v1485', require('./src/routes/v1485'));
	router.use('/api/v1486', require('./src/routes/v1486'));
	router.use('/api/v1487', require('./src/routes/v1487'));
	router.use('/api/v1488', require('./src/routes/v1488'));
	router.use('/api/v1489', require('./src/routes/v1489'));
	router.use('/api/v1490', require('./src/routes/v1490'));
	router.use('/api/v1491', require('./src/routes/v1491'));
	router.use('/api/v1492', require('./src/routes/v1492'));
	router.use('/api/v1493', require('./src/routes/v1493'));
	router.use('/api/v1494', require('./src/routes/v1494'));
	router.use('/api/v1495', require('./src/routes/v1495'));
	router.use('/api/v1496', require('./src/routes/v1496'));
	router.use('/api/v1497', require('./src/routes/v1497'));
	router.use('/api/v1498', require('./src/routes/v1498'));
	router.use('/api/v1499', require('./src/routes/v1499'));
	router.use('/api/v1500', require('./src/routes/v1500'));
	router.use('/api/v1501', require('./src/routes/v1501'));
	router.use('/api/v1502', require('./src/routes/v1502'));
	router.use('/api/v1503', require('./src/routes/v1503'));
	router.use('/api/v1504', require('./src/routes/v1504'));
	router.use('/api/v1505', require('./src/routes/v1505'));
	router.use('/api/v1506', require('./src/routes/v1506'));
	router.use('/api/v1507', require('./src/routes/v1507'));
	router.use('/api/v1508', require('./src/routes/v1508'));
	router.use('/api/v1509', require('./src/routes/v1509'));
	router.use('/api/v1510', require('./src/routes/v1510'));
	router.use('/api/v1511', require('./src/routes/v1511'));
	router.use('/api/v1512', require('./src/routes/v1512'));
	router.use('/api/v1513', require('./src/routes/v1513'));
	router.use('/api/v1514', require('./src/routes/v1514'));
	router.use('/api/v1515', require('./src/routes/v1515'));
	router.use('/api/v1516', require('./src/routes/v1516'));
	router.use('/api/v1517', require('./src/routes/v1517'));
	router.use('/api/v1518', require('./src/routes/v1518'));
	router.use('/api/v1519', require('./src/routes/v1519'));
	router.use('/api/v1520', require('./src/routes/v1520'));
	router.use('/api/v1521', require('./src/routes/v1521'));
	router.use('/api/v1522', require('./src/routes/v1522'));
	router.use('/api/v1523', require('./src/routes/v1523'));
	router.use('/api/v1524', require('./src/routes/v1524'));
	router.use('/api/v1525', require('./src/routes/v1525'));
	router.use('/api/v1526', require('./src/routes/v1526'));
	router.use('/api/v1527', require('./src/routes/v1527'));
	router.use('/api/v1528', require('./src/routes/v1528'));
	router.use('/api/v1529', require('./src/routes/v1529'));
	router.use('/api/v1530', require('./src/routes/v1530'));
	router.use('/api/v1531', require('./src/routes/v1531'));
	router.use('/api/v1532', require('./src/routes/v1532'));
	router.use('/api/v1533', require('./src/routes/v1533'));
	router.use('/api/v1534', require('./src/routes/v1534'));
	router.use('/api/v1535', require('./src/routes/v1535'));
	router.use('/api/v1536', require('./src/routes/v1536'));
	router.use('/api/v1537', require('./src/routes/v1537'));
	router.use('/api/v1538', require('./src/routes/v1538'));
	router.use('/api/v1539', require('./src/routes/v1539'));
	router.use('/api/v1540', require('./src/routes/v1540'));
	router.use('/api/v1541', require('./src/routes/v1541'));
	router.use('/api/v1542', require('./src/routes/v1542'));
	router.use('/api/v1543', require('./src/routes/v1543'));
	router.use('/api/v1544', require('./src/routes/v1544'));
	router.use('/api/v1545', require('./src/routes/v1545'));
	router.use('/api/v1546', require('./src/routes/v1546'));
	router.use('/api/v1547', require('./src/routes/v1547'));
	router.use('/api/v1548', require('./src/routes/v1548'));
	router.use('/api/v1549', require('./src/routes/v1549'));
	router.use('/api/v1550', require('./src/routes/v1550'));
	router.use('/api/v1551', require('./src/routes/v1551'));
	router.use('/api/v1552', require('./src/routes/v1552'));
	router.use('/api/v1553', require('./src/routes/v1553'));
	router.use('/api/v1554', require('./src/routes/v1554'));
	router.use('/api/v1555', require('./src/routes/v1555'));
	router.use('/api/v1556', require('./src/routes/v1556'));
	router.use('/api/v1557', require('./src/routes/v1557'));
	router.use('/api/v1558', require('./src/routes/v1558'));
	router.use('/api/v1559', require('./src/routes/v1559'));
	router.use('/api/v1560', require('./src/routes/v1560'));
	router.use('/api/v1561', require('./src/routes/v1561'));
	router.use('/api/v1562', require('./src/routes/v1562'));
	router.use('/api/v1563', require('./src/routes/v1563'));
	router.use('/api/v1564', require('./src/routes/v1564'));
	router.use('/api/v1565', require('./src/routes/v1565'));
	router.use('/api/v1566', require('./src/routes/v1566'));
	router.use('/api/v1567', require('./src/routes/v1567'));
	router.use('/api/v1568', require('./src/routes/v1568'));
	router.use('/api/v1569', require('./src/routes/v1569'));
	router.use('/api/v1570', require('./src/routes/v1570'));
	router.use('/api/v1571', require('./src/routes/v1571'));
	router.use('/api/v1572', require('./src/routes/v1572'));
	router.use('/api/v1573', require('./src/routes/v1573'));
	router.use('/api/v1574', require('./src/routes/v1574'));
	router.use('/api/v1575', require('./src/routes/v1575'));
	router.use('/api/v1576', require('./src/routes/v1576'));
	router.use('/api/v1577', require('./src/routes/v1577'));
	router.use('/api/v1578', require('./src/routes/v1578'));
	router.use('/api/v1579', require('./src/routes/v1579'));
	router.use('/api/v1580', require('./src/routes/v1580'));
	router.use('/api/v1581', require('./src/routes/v1581'));
	router.use('/api/v1582', require('./src/routes/v1582'));
	router.use('/api/v1583', require('./src/routes/v1583'));
	router.use('/api/v1584', require('./src/routes/v1584'));
	router.use('/api/v1585', require('./src/routes/v1585'));
	router.use('/api/v1586', require('./src/routes/v1586'));
	router.use('/api/v1587', require('./src/routes/v1587'));
	router.use('/api/v1588', require('./src/routes/v1588'));
	router.use('/api/v1589', require('./src/routes/v1589'));
	router.use('/api/v1590', require('./src/routes/v1590'));
	router.use('/api/v1591', require('./src/routes/v1591'));
	router.use('/api/v1592', require('./src/routes/v1592'));
	router.use('/api/v1593', require('./src/routes/v1593'));
	router.use('/api/v1594', require('./src/routes/v1594'));
	router.use('/api/v1595', require('./src/routes/v1595'));
	router.use('/api/v1596', require('./src/routes/v1596'));
	router.use('/api/v1597', require('./src/routes/v1597'));
	router.use('/api/v1598', require('./src/routes/v1598'));
	router.use('/api/v1599', require('./src/routes/v1599'));
	router.use('/api/v1600', require('./src/routes/v1600'));
	router.use('/api/v1601', require('./src/routes/v1601'));
	router.use('/api/v1602', require('./src/routes/v1602'));
	router.use('/api/v1603', require('./src/routes/v1603'));
	router.use('/api/v1604', require('./src/routes/v1604'));
	router.use('/api/v1605', require('./src/routes/v1605'));
	router.use('/api/v1606', require('./src/routes/v1606'));
	router.use('/api/v1607', require('./src/routes/v1607'));
	router.use('/api/v1608', require('./src/routes/v1608'));
	router.use('/api/v1609', require('./src/routes/v1609'));
	router.use('/api/v1610', require('./src/routes/v1610'));
	router.use('/api/v1611', require('./src/routes/v1611'));
	router.use('/api/v1612', require('./src/routes/v1612'));
	router.use('/api/v1613', require('./src/routes/v1613'));
	router.use('/api/v1614', require('./src/routes/v1614'));
	router.use('/api/v1615', require('./src/routes/v1615'));
	router.use('/api/v1616', require('./src/routes/v1616'));
	router.use('/api/v1617', require('./src/routes/v1617'));
	router.use('/api/v1618', require('./src/routes/v1618'));
	router.use('/api/v1619', require('./src/routes/v1619'));
	router.use('/api/v1620', require('./src/routes/v1620'));
	router.use('/api/v1621', require('./src/routes/v1621'));
	router.use('/api/v1622', require('./src/routes/v1622'));
	router.use('/api/v1623', require('./src/routes/v1623'));
	router.use('/api/v1624', require('./src/routes/v1624'));
	router.use('/api/v1625', require('./src/routes/v1625'));
	router.use('/api/v1626', require('./src/routes/v1626'));
	router.use('/api/v1627', require('./src/routes/v1627'));
	router.use('/api/v1628', require('./src/routes/v1628'));
	router.use('/api/v1629', require('./src/routes/v1629'));
	router.use('/api/v1630', require('./src/routes/v1630'));
	router.use('/api/v1631', require('./src/routes/v1631'));
	router.use('/api/v1632', require('./src/routes/v1632'));
	router.use('/api/v1633', require('./src/routes/v1633'));
	router.use('/api/v1634', require('./src/routes/v1634'));
	router.use('/api/v1635', require('./src/routes/v1635'));
	router.use('/api/v1636', require('./src/routes/v1636'));
	router.use('/api/v1637', require('./src/routes/v1637'));
	router.use('/api/v1638', require('./src/routes/v1638'));
	router.use('/api/v1639', require('./src/routes/v1639'));
	router.use('/api/v1640', require('./src/routes/v1640'));
	router.use('/api/v1641', require('./src/routes/v1641'));
	router.use('/api/v1642', require('./src/routes/v1642'));
	router.use('/api/v1643', require('./src/routes/v1643'));
	router.use('/api/v1644', require('./src/routes/v1644'));
	router.use('/api/v1645', require('./src/routes/v1645'));
	router.use('/api/v1646', require('./src/routes/v1646'));
	router.use('/api/v1647', require('./src/routes/v1647'));
	router.use('/api/v1648', require('./src/routes/v1648'));
	router.use('/api/v1649', require('./src/routes/v1649'));
	router.use('/api/v1650', require('./src/routes/v1650'));
	router.use('/api/v1651', require('./src/routes/v1651'));
	router.use('/api/v1652', require('./src/routes/v1652'));
	router.use('/api/v1653', require('./src/routes/v1653'));
	router.use('/api/v1654', require('./src/routes/v1654'));
	router.use('/api/v1655', require('./src/routes/v1655'));
	router.use('/api/v1656', require('./src/routes/v1656'));
	router.use('/api/v1657', require('./src/routes/v1657'));
	router.use('/api/v1658', require('./src/routes/v1658'));
	router.use('/api/v1659', require('./src/routes/v1659'));
	router.use('/api/v1660', require('./src/routes/v1660'));
	router.use('/api/v1661', require('./src/routes/v1661'));
	router.use('/api/v1662', require('./src/routes/v1662'));
	router.use('/api/v1663', require('./src/routes/v1663'));
	router.use('/api/v1664', require('./src/routes/v1664'));
	router.use('/api/v1665', require('./src/routes/v1665'));
	router.use('/api/v1666', require('./src/routes/v1666'));
	router.use('/api/v1667', require('./src/routes/v1667'));
	router.use('/api/v1668', require('./src/routes/v1668'));
	router.use('/api/v1669', require('./src/routes/v1669'));
	router.use('/api/v1670', require('./src/routes/v1670'));
	router.use('/api/v1671', require('./src/routes/v1671'));
	router.use('/api/v1672', require('./src/routes/v1672'));
	router.use('/api/v1673', require('./src/routes/v1673'));
	router.use('/api/v1674', require('./src/routes/v1674'));
	router.use('/api/v1675', require('./src/routes/v1675'));
	router.use('/api/v1676', require('./src/routes/v1676'));
	router.use('/api/v1677', require('./src/routes/v1677'));
	router.use('/api/v1678', require('./src/routes/v1678'));
	router.use('/api/v1679', require('./src/routes/v1679'));
	router.use('/api/v1680', require('./src/routes/v1680'));
	router.use('/api/v1681', require('./src/routes/v1681'));
	router.use('/api/v1682', require('./src/routes/v1682'));
	router.use('/api/v1683', require('./src/routes/v1683'));
	router.use('/api/v1684', require('./src/routes/v1684'));
	router.use('/api/v1685', require('./src/routes/v1685'));
	router.use('/api/v1686', require('./src/routes/v1686'));
	router.use('/api/v1687', require('./src/routes/v1687'));
	router.use('/api/v1688', require('./src/routes/v1688'));
	router.use('/api/v1689', require('./src/routes/v1689'));
	router.use('/api/v1690', require('./src/routes/v1690'));
	router.use('/api/v1691', require('./src/routes/v1691'));
	router.use('/api/v1692', require('./src/routes/v1692'));
	router.use('/api/v1693', require('./src/routes/v1693'));
	router.use('/api/v1694', require('./src/routes/v1694'));
	router.use('/api/v1695', require('./src/routes/v1695'));
	router.use('/api/v1696', require('./src/routes/v1696'));
	router.use('/api/v1697', require('./src/routes/v1697'));
	router.use('/api/v1698', require('./src/routes/v1698'));
	router.use('/api/v1699', require('./src/routes/v1699'));
	router.use('/api/v1700', require('./src/routes/v1700'));
	router.use('/api/v1701', require('./src/routes/v1701'));
	router.use('/api/v1702', require('./src/routes/v1702'));
	router.use('/api/v1703', require('./src/routes/v1703'));
	router.use('/api/v1704', require('./src/routes/v1704'));
	router.use('/api/v1705', require('./src/routes/v1705'));
	router.use('/api/v1706', require('./src/routes/v1706'));
	router.use('/api/v1707', require('./src/routes/v1707'));
	router.use('/api/v1708', require('./src/routes/v1708'));
	router.use('/api/v1709', require('./src/routes/v1709'));
	router.use('/api/v1710', require('./src/routes/v1710'));
	router.use('/api/v1711', require('./src/routes/v1711'));
	router.use('/api/v1712', require('./src/routes/v1712'));
	router.use('/api/v1713', require('./src/routes/v1713'));
	router.use('/api/v1714', require('./src/routes/v1714'));
	router.use('/api/v1715', require('./src/routes/v1715'));
	router.use('/api/v1716', require('./src/routes/v1716'));
	router.use('/api/v1717', require('./src/routes/v1717'));
	router.use('/api/v1718', require('./src/routes/v1718'));
	router.use('/api/v1719', require('./src/routes/v1719'));
	router.use('/api/v1720', require('./src/routes/v1720'));
	router.use('/api/v1721', require('./src/routes/v1721'));
	router.use('/api/v1722', require('./src/routes/v1722'));
	router.use('/api/v1723', require('./src/routes/v1723'));
	router.use('/api/v1724', require('./src/routes/v1724'));
	router.use('/api/v1725', require('./src/routes/v1725'));
	router.use('/api/v1726', require('./src/routes/v1726'));
	router.use('/api/v1727', require('./src/routes/v1727'));
	router.use('/api/v1728', require('./src/routes/v1728'));
	router.use('/api/v1729', require('./src/routes/v1729'));
	router.use('/api/v1730', require('./src/routes/v1730'));
	router.use('/api/v1731', require('./src/routes/v1731'));
	router.use('/api/v1732', require('./src/routes/v1732'));
	router.use('/api/v1733', require('./src/routes/v1733'));
	router.use('/api/v1734', require('./src/routes/v1734'));
	router.use('/api/v1735', require('./src/routes/v1735'));
	router.use('/api/v1736', require('./src/routes/v1736'));
	router.use('/api/v1737', require('./src/routes/v1737'));
	router.use('/api/v1738', require('./src/routes/v1738'));
	router.use('/api/v1739', require('./src/routes/v1739'));
	router.use('/api/v1740', require('./src/routes/v1740'));
	router.use('/api/v1741', require('./src/routes/v1741'));
	router.use('/api/v1742', require('./src/routes/v1742'));
	router.use('/api/v1743', require('./src/routes/v1743'));
	router.use('/api/v1744', require('./src/routes/v1744'));
	router.use('/api/v1745', require('./src/routes/v1745'));
	router.use('/api/v1746', require('./src/routes/v1746'));
	router.use('/api/v1747', require('./src/routes/v1747'));
	router.use('/api/v1748', require('./src/routes/v1748'));
	router.use('/api/v1749', require('./src/routes/v1749'));
	router.use('/api/v1750', require('./src/routes/v1750'));
	router.use('/api/v1751', require('./src/routes/v1751'));
	router.use('/api/v1752', require('./src/routes/v1752'));
	router.use('/api/v1753', require('./src/routes/v1753'));
	router.use('/api/v1754', require('./src/routes/v1754'));
	router.use('/api/v1755', require('./src/routes/v1755'));
	router.use('/api/v1756', require('./src/routes/v1756'));
	router.use('/api/v1757', require('./src/routes/v1757'));
	router.use('/api/v1758', require('./src/routes/v1758'));
	router.use('/api/v1759', require('./src/routes/v1759'));
	router.use('/api/v1760', require('./src/routes/v1760'));
	router.use('/api/v1761', require('./src/routes/v1761'));
	router.use('/api/v1762', require('./src/routes/v1762'));
	router.use('/api/v1763', require('./src/routes/v1763'));
	router.use('/api/v1764', require('./src/routes/v1764'));
	router.use('/api/v1765', require('./src/routes/v1765'));
	router.use('/api/v1766', require('./src/routes/v1766'));
	router.use('/api/v1767', require('./src/routes/v1767'));
	router.use('/api/v1768', require('./src/routes/v1768'));
	router.use('/api/v1769', require('./src/routes/v1769'));
	router.use('/api/v1770', require('./src/routes/v1770'));
	router.use('/api/v1771', require('./src/routes/v1771'));
	router.use('/api/v1772', require('./src/routes/v1772'));
	router.use('/api/v1773', require('./src/routes/v1773'));
	router.use('/api/v1774', require('./src/routes/v1774'));
	router.use('/api/v1775', require('./src/routes/v1775'));
	router.use('/api/v1776', require('./src/routes/v1776'));
	router.use('/api/v1777', require('./src/routes/v1777'));
	router.use('/api/v1778', require('./src/routes/v1778'));
	router.use('/api/v1779', require('./src/routes/v1779'));
	router.use('/api/v1780', require('./src/routes/v1780'));
	router.use('/api/v1781', require('./src/routes/v1781'));
	router.use('/api/v1782', require('./src/routes/v1782'));
	router.use('/api/v1783', require('./src/routes/v1783'));
	router.use('/api/v1784', require('./src/routes/v1784'));
	router.use('/api/v1785', require('./src/routes/v1785'));
	router.use('/api/v1786', require('./src/routes/v1786'));
	router.use('/api/v1787', require('./src/routes/v1787'));
	router.use('/api/v1788', require('./src/routes/v1788'));
	router.use('/api/v1789', require('./src/routes/v1789'));
	router.use('/api/v1790', require('./src/routes/v1790'));
	router.use('/api/v1791', require('./src/routes/v1791'));
	router.use('/api/v1792', require('./src/routes/v1792'));
	router.use('/api/v1793', require('./src/routes/v1793'));
	router.use('/api/v1794', require('./src/routes/v1794'));
	router.use('/api/v1795', require('./src/routes/v1795'));
	router.use('/api/v1796', require('./src/routes/v1796'));
	router.use('/api/v1797', require('./src/routes/v1797'));
	router.use('/api/v1798', require('./src/routes/v1798'));
	router.use('/api/v1799', require('./src/routes/v1799'));
	router.use('/api/v1800', require('./src/routes/v1800'));
	router.use('/api/v1801', require('./src/routes/v1801'));
	router.use('/api/v1802', require('./src/routes/v1802'));
	router.use('/api/v1803', require('./src/routes/v1803'));
	router.use('/api/v1804', require('./src/routes/v1804'));
	router.use('/api/v1805', require('./src/routes/v1805'));
	router.use('/api/v1806', require('./src/routes/v1806'));
	router.use('/api/v1807', require('./src/routes/v1807'));
	router.use('/api/v1808', require('./src/routes/v1808'));
	router.use('/api/v1809', require('./src/routes/v1809'));
	router.use('/api/v1810', require('./src/routes/v1810'));
	router.use('/api/v1811', require('./src/routes/v1811'));
	router.use('/api/v1812', require('./src/routes/v1812'));
	router.use('/api/v1813', require('./src/routes/v1813'));
	router.use('/api/v1814', require('./src/routes/v1814'));
	router.use('/api/v1815', require('./src/routes/v1815'));
	router.use('/api/v1816', require('./src/routes/v1816'));
	router.use('/api/v1817', require('./src/routes/v1817'));
	router.use('/api/v1818', require('./src/routes/v1818'));
	router.use('/api/v1819', require('./src/routes/v1819'));
	router.use('/api/v1820', require('./src/routes/v1820'));
	router.use('/api/v1821', require('./src/routes/v1821'));
	router.use('/api/v1822', require('./src/routes/v1822'));
	router.use('/api/v1823', require('./src/routes/v1823'));
	router.use('/api/v1824', require('./src/routes/v1824'));
	router.use('/api/v1825', require('./src/routes/v1825'));
	router.use('/api/v1826', require('./src/routes/v1826'));
	router.use('/api/v1827', require('./src/routes/v1827'));
	router.use('/api/v1828', require('./src/routes/v1828'));
	router.use('/api/v1829', require('./src/routes/v1829'));
	router.use('/api/v1830', require('./src/routes/v1830'));
	router.use('/api/v1831', require('./src/routes/v1831'));
	router.use('/api/v1832', require('./src/routes/v1832'));
	router.use('/api/v1833', require('./src/routes/v1833'));
	router.use('/api/v1834', require('./src/routes/v1834'));
	router.use('/api/v1835', require('./src/routes/v1835'));
	router.use('/api/v1836', require('./src/routes/v1836'));
	router.use('/api/v1837', require('./src/routes/v1837'));
	router.use('/api/v1838', require('./src/routes/v1838'));
	router.use('/api/v1839', require('./src/routes/v1839'));
	router.use('/api/v1840', require('./src/routes/v1840'));
	router.use('/api/v1841', require('./src/routes/v1841'));
	router.use('/api/v1842', require('./src/routes/v1842'));
	router.use('/api/v1843', require('./src/routes/v1843'));
	router.use('/api/v1844', require('./src/routes/v1844'));
	router.use('/api/v1845', require('./src/routes/v1845'));
	router.use('/api/v1846', require('./src/routes/v1846'));
	router.use('/api/v1847', require('./src/routes/v1847'));
	router.use('/api/v1848', require('./src/routes/v1848'));
	router.use('/api/v1849', require('./src/routes/v1849'));
	router.use('/api/v1850', require('./src/routes/v1850'));
	router.use('/api/v1851', require('./src/routes/v1851'));
	router.use('/api/v1852', require('./src/routes/v1852'));
	router.use('/api/v1853', require('./src/routes/v1853'));
	router.use('/api/v1854', require('./src/routes/v1854'));
	router.use('/api/v1855', require('./src/routes/v1855'));
	router.use('/api/v1856', require('./src/routes/v1856'));
	router.use('/api/v1857', require('./src/routes/v1857'));
	router.use('/api/v1858', require('./src/routes/v1858'));
	router.use('/api/v1859', require('./src/routes/v1859'));
	router.use('/api/v1860', require('./src/routes/v1860'));
	router.use('/api/v1861', require('./src/routes/v1861'));
	router.use('/api/v1862', require('./src/routes/v1862'));
	router.use('/api/v1863', require('./src/routes/v1863'));
	router.use('/api/v1864', require('./src/routes/v1864'));
	router.use('/api/v1865', require('./src/routes/v1865'));
	router.use('/api/v1866', require('./src/routes/v1866'));
	router.use('/api/v1867', require('./src/routes/v1867'));
	router.use('/api/v1868', require('./src/routes/v1868'));
	router.use('/api/v1869', require('./src/routes/v1869'));
	router.use('/api/v1870', require('./src/routes/v1870'));
	router.use('/api/v1871', require('./src/routes/v1871'));
	router.use('/api/v1872', require('./src/routes/v1872'));
	router.use('/api/v1873', require('./src/routes/v1873'));
	router.use('/api/v1874', require('./src/routes/v1874'));
	router.use('/api/v1875', require('./src/routes/v1875'));
	router.use('/api/v1876', require('./src/routes/v1876'));
	router.use('/api/v1877', require('./src/routes/v1877'));
	router.use('/api/v1878', require('./src/routes/v1878'));
	router.use('/api/v1879', require('./src/routes/v1879'));
	router.use('/api/v1880', require('./src/routes/v1880'));
	router.use('/api/v1881', require('./src/routes/v1881'));
	router.use('/api/v1882', require('./src/routes/v1882'));
	router.use('/api/v1883', require('./src/routes/v1883'));
	router.use('/api/v1884', require('./src/routes/v1884'));
	router.use('/api/v1885', require('./src/routes/v1885'));
	router.use('/api/v1886', require('./src/routes/v1886'));
	router.use('/api/v1887', require('./src/routes/v1887'));
	router.use('/api/v1888', require('./src/routes/v1888'));
	router.use('/api/v1889', require('./src/routes/v1889'));
	router.use('/api/v1890', require('./src/routes/v1890'));
	router.use('/api/v1891', require('./src/routes/v1891'));
	router.use('/api/v1892', require('./src/routes/v1892'));
	router.use('/api/v1893', require('./src/routes/v1893'));
	router.use('/api/v1894', require('./src/routes/v1894'));
	router.use('/api/v1895', require('./src/routes/v1895'));
	router.use('/api/v1896', require('./src/routes/v1896'));
	router.use('/api/v1897', require('./src/routes/v1897'));
	router.use('/api/v1898', require('./src/routes/v1898'));
	router.use('/api/v1899', require('./src/routes/v1899'));
	router.use('/api/v1900', require('./src/routes/v1900'));
	router.use('/api/v1901', require('./src/routes/v1901'));
	router.use('/api/v1902', require('./src/routes/v1902'));
	router.use('/api/v1903', require('./src/routes/v1903'));
	router.use('/api/v1904', require('./src/routes/v1904'));
	router.use('/api/v1905', require('./src/routes/v1905'));
	router.use('/api/v1906', require('./src/routes/v1906'));
	router.use('/api/v1907', require('./src/routes/v1907'));
	router.use('/api/v1908', require('./src/routes/v1908'));
	router.use('/api/v1909', require('./src/routes/v1909'));
	router.use('/api/v1910', require('./src/routes/v1910'));
	router.use('/api/v1911', require('./src/routes/v1911'));
	router.use('/api/v1912', require('./src/routes/v1912'));
	router.use('/api/v1913', require('./src/routes/v1913'));
	router.use('/api/v1914', require('./src/routes/v1914'));
	router.use('/api/v1915', require('./src/routes/v1915'));
	router.use('/api/v1916', require('./src/routes/v1916'));
	router.use('/api/v1917', require('./src/routes/v1917'));
	router.use('/api/v1918', require('./src/routes/v1918'));
	router.use('/api/v1919', require('./src/routes/v1919'));
	router.use('/api/v1920', require('./src/routes/v1920'));
	router.use('/api/v1921', require('./src/routes/v1921'));
	router.use('/api/v1922', require('./src/routes/v1922'));
	router.use('/api/v1923', require('./src/routes/v1923'));
	router.use('/api/v1924', require('./src/routes/v1924'));
	router.use('/api/v1925', require('./src/routes/v1925'));
	router.use('/api/v1926', require('./src/routes/v1926'));
	router.use('/api/v1927', require('./src/routes/v1927'));
	router.use('/api/v1928', require('./src/routes/v1928'));
	router.use('/api/v1929', require('./src/routes/v1929'));
	router.use('/api/v1930', require('./src/routes/v1930'));
	router.use('/api/v1931', require('./src/routes/v1931'));
	router.use('/api/v1932', require('./src/routes/v1932'));
	router.use('/api/v1933', require('./src/routes/v1933'));
	router.use('/api/v1934', require('./src/routes/v1934'));
	router.use('/api/v1935', require('./src/routes/v1935'));
	router.use('/api/v1936', require('./src/routes/v1936'));
	router.use('/api/v1937', require('./src/routes/v1937'));
	router.use('/api/v1938', require('./src/routes/v1938'));
	router.use('/api/v1939', require('./src/routes/v1939'));
	router.use('/api/v1940', require('./src/routes/v1940'));
	router.use('/api/v1941', require('./src/routes/v1941'));
	router.use('/api/v1942', require('./src/routes/v1942'));
	router.use('/api/v1943', require('./src/routes/v1943'));
	router.use('/api/v1944', require('./src/routes/v1944'));
	router.use('/api/v1945', require('./src/routes/v1945'));
	router.use('/api/v1946', require('./src/routes/v1946'));
	router.use('/api/v1947', require('./src/routes/v1947'));
	router.use('/api/v1948', require('./src/routes/v1948'));
	router.use('/api/v1949', require('./src/routes/v1949'));
	router.use('/api/v1950', require('./src/routes/v1950'));
	router.use('/api/v1951', require('./src/routes/v1951'));
	router.use('/api/v1952', require('./src/routes/v1952'));
	router.use('/api/v1953', require('./src/routes/v1953'));
	router.use('/api/v1954', require('./src/routes/v1954'));
	router.use('/api/v1955', require('./src/routes/v1955'));
	router.use('/api/v1956', require('./src/routes/v1956'));
	router.use('/api/v1957', require('./src/routes/v1957'));
	router.use('/api/v1958', require('./src/routes/v1958'));
	router.use('/api/v1959', require('./src/routes/v1959'));
	router.use('/api/v1960', require('./src/routes/v1960'));
	router.use('/api/v1961', require('./src/routes/v1961'));
	router.use('/api/v1962', require('./src/routes/v1962'));
	router.use('/api/v1963', require('./src/routes/v1963'));
	router.use('/api/v1964', require('./src/routes/v1964'));
	router.use('/api/v1965', require('./src/routes/v1965'));
	router.use('/api/v1966', require('./src/routes/v1966'));
	router.use('/api/v1967', require('./src/routes/v1967'));
	router.use('/api/v1968', require('./src/routes/v1968'));
	router.use('/api/v1969', require('./src/routes/v1969'));
	router.use('/api/v1970', require('./src/routes/v1970'));
	router.use('/api/v1971', require('./src/routes/v1971'));
	router.use('/api/v1972', require('./src/routes/v1972'));
	router.use('/api/v1973', require('./src/routes/v1973'));
	router.use('/api/v1974', require('./src/routes/v1974'));
	router.use('/api/v1975', require('./src/routes/v1975'));
	router.use('/api/v1976', require('./src/routes/v1976'));
	router.use('/api/v1977', require('./src/routes/v1977'));
	router.use('/api/v1978', require('./src/routes/v1978'));
	router.use('/api/v1979', require('./src/routes/v1979'));
	router.use('/api/v1980', require('./src/routes/v1980'));
	router.use('/api/v1981', require('./src/routes/v1981'));
	router.use('/api/v1982', require('./src/routes/v1982'));
	router.use('/api/v1983', require('./src/routes/v1983'));
	router.use('/api/v1984', require('./src/routes/v1984'));
	router.use('/api/v1985', require('./src/routes/v1985'));
	router.use('/api/v1986', require('./src/routes/v1986'));
	router.use('/api/v1987', require('./src/routes/v1987'));
	router.use('/api/v1988', require('./src/routes/v1988'));
	router.use('/api/v1989', require('./src/routes/v1989'));
	router.use('/api/v1990', require('./src/routes/v1990'));
	router.use('/api/v1991', require('./src/routes/v1991'));
	router.use('/api/v1992', require('./src/routes/v1992'));
	router.use('/api/v1993', require('./src/routes/v1993'));
	router.use('/api/v1994', require('./src/routes/v1994'));
	router.use('/api/v1995', require('./src/routes/v1995'));
	router.use('/api/v1996', require('./src/routes/v1996'));
	router.use('/api/v1997', require('./src/routes/v1997'));
	router.use('/api/v1998', require('./src/routes/v1998'));
	router.use('/api/v1999', require('./src/routes/v1999'));
	router.use('/api/v2000', require('./src/routes/v2000'));
	router.use('/api/v2001', require('./src/routes/v2001'));
	router.use('/api/v2002', require('./src/routes/v2002'));
	router.use('/api/v2003', require('./src/routes/v2003'));
	router.use('/api/v2004', require('./src/routes/v2004'));
	router.use('/api/v2005', require('./src/routes/v2005'));
	router.use('/api/v2006', require('./src/routes/v2006'));
	router.use('/api/v2007', require('./src/routes/v2007'));
	router.use('/api/v2008', require('./src/routes/v2008'));
	router.use('/api/v2009', require('./src/routes/v2009'));
	router.use('/api/v2010', require('./src/routes/v2010'));
	router.use('/api/v2011', require('./src/routes/v2011'));
	router.use('/api/v2012', require('./src/routes/v2012'));
	router.use('/api/v2013', require('./src/routes/v2013'));
	router.use('/api/v2014', require('./src/routes/v2014'));
	router.use('/api/v2015', require('./src/routes/v2015'));
	router.use('/api/v2016', require('./src/routes/v2016'));
	router.use('/api/v2017', require('./src/routes/v2017'));
	router.use('/api/v2018', require('./src/routes/v2018'));
	router.use('/api/v2019', require('./src/routes/v2019'));
	router.use('/api/v2020', require('./src/routes/v2020'));
	router.use('/api/v2021', require('./src/routes/v2021'));
	router.use('/api/v2022', require('./src/routes/v2022'));
	router.use('/api/v2023', require('./src/routes/v2023'));
	router.use('/api/v2024', require('./src/routes/v2024'));
	router.use('/api/v2025', require('./src/routes/v2025'));
	router.use('/api/v2026', require('./src/routes/v2026'));
	router.use('/api/v2027', require('./src/routes/v2027'));
	router.use('/api/v2028', require('./src/routes/v2028'));
	router.use('/api/v2029', require('./src/routes/v2029'));
	router.use('/api/v2030', require('./src/routes/v2030'));
	router.use('/api/v2031', require('./src/routes/v2031'));
	router.use('/api/v2032', require('./src/routes/v2032'));
	router.use('/api/v2033', require('./src/routes/v2033'));
	router.use('/api/v2034', require('./src/routes/v2034'));
	router.use('/api/v2035', require('./src/routes/v2035'));
	router.use('/api/v2036', require('./src/routes/v2036'));
	router.use('/api/v2037', require('./src/routes/v2037'));
	router.use('/api/v2038', require('./src/routes/v2038'));
	router.use('/api/v2039', require('./src/routes/v2039'));
	router.use('/api/v2040', require('./src/routes/v2040'));
	router.use('/api/v2041', require('./src/routes/v2041'));
	router.use('/api/v2042', require('./src/routes/v2042'));
	router.use('/api/v2043', require('./src/routes/v2043'));
	router.use('/api/v2044', require('./src/routes/v2044'));
	router.use('/api/v2045', require('./src/routes/v2045'));
	router.use('/api/v2046', require('./src/routes/v2046'));
	router.use('/api/v2047', require('./src/routes/v2047'));
	router.use('/api/v2048', require('./src/routes/v2048'));
	router.use('/api/v2049', require('./src/routes/v2049'));
	router.use('/api/v2050', require('./src/routes/v2050'));
	router.use('/api/v2051', require('./src/routes/v2051'));
	router.use('/api/v2052', require('./src/routes/v2052'));
	router.use('/api/v2053', require('./src/routes/v2053'));
	router.use('/api/v2054', require('./src/routes/v2054'));
	router.use('/api/v2055', require('./src/routes/v2055'));
	router.use('/api/v2056', require('./src/routes/v2056'));
	router.use('/api/v2057', require('./src/routes/v2057'));
	router.use('/api/v2058', require('./src/routes/v2058'));
	router.use('/api/v2059', require('./src/routes/v2059'));
	router.use('/api/v2060', require('./src/routes/v2060'));
	router.use('/api/v2061', require('./src/routes/v2061'));
	router.use('/api/v2062', require('./src/routes/v2062'));
	router.use('/api/v2063', require('./src/routes/v2063'));
	router.use('/api/v2064', require('./src/routes/v2064'));
	router.use('/api/v2065', require('./src/routes/v2065'));
	router.use('/api/v2066', require('./src/routes/v2066'));
	router.use('/api/v2067', require('./src/routes/v2067'));
	router.use('/api/v2068', require('./src/routes/v2068'));
	router.use('/api/v2069', require('./src/routes/v2069'));
	router.use('/api/v2070', require('./src/routes/v2070'));
	router.use('/api/v2071', require('./src/routes/v2071'));
	router.use('/api/v2072', require('./src/routes/v2072'));
	router.use('/api/v2073', require('./src/routes/v2073'));
	router.use('/api/v2074', require('./src/routes/v2074'));
	router.use('/api/v2075', require('./src/routes/v2075'));
	router.use('/api/v2076', require('./src/routes/v2076'));
	router.use('/api/v2077', require('./src/routes/v2077'));
	router.use('/api/v2078', require('./src/routes/v2078'));
	router.use('/api/v2079', require('./src/routes/v2079'));
	router.use('/api/v2080', require('./src/routes/v2080'));
	router.use('/api/v2081', require('./src/routes/v2081'));
	router.use('/api/v2082', require('./src/routes/v2082'));
	router.use('/api/v2083', require('./src/routes/v2083'));
	router.use('/api/v2084', require('./src/routes/v2084'));
	router.use('/api/v2085', require('./src/routes/v2085'));
	router.use('/api/v2086', require('./src/routes/v2086'));
	router.use('/api/v2087', require('./src/routes/v2087'));
	router.use('/api/v2088', require('./src/routes/v2088'));
	router.use('/api/v2089', require('./src/routes/v2089'));
	router.use('/api/v2090', require('./src/routes/v2090'));
	router.use('/api/v2091', require('./src/routes/v2091'));
	router.use('/api/v2092', require('./src/routes/v2092'));
	router.use('/api/v2093', require('./src/routes/v2093'));
	router.use('/api/v2094', require('./src/routes/v2094'));
	router.use('/api/v2095', require('./src/routes/v2095'));
	router.use('/api/v2096', require('./src/routes/v2096'));
	router.use('/api/v2097', require('./src/routes/v2097'));
	router.use('/api/v2098', require('./src/routes/v2098'));
	router.use('/api/v2099', require('./src/routes/v2099'));
	router.use('/api/v2100', require('./src/routes/v2100'));
	router.use('/api/v2101', require('./src/routes/v2101'));
	router.use('/api/v2102', require('./src/routes/v2102'));
	router.use('/api/v2103', require('./src/routes/v2103'));
	router.use('/api/v2104', require('./src/routes/v2104'));
	router.use('/api/v2105', require('./src/routes/v2105'));
	router.use('/api/v2106', require('./src/routes/v2106'));
	router.use('/api/v2107', require('./src/routes/v2107'));
	router.use('/api/v2108', require('./src/routes/v2108'));
	router.use('/api/v2109', require('./src/routes/v2109'));
	router.use('/api/v2110', require('./src/routes/v2110'));
	router.use('/api/v2111', require('./src/routes/v2111'));
	router.use('/api/v2112', require('./src/routes/v2112'));
	router.use('/api/v2113', require('./src/routes/v2113'));
	router.use('/api/v2114', require('./src/routes/v2114'));
	router.use('/api/v2115', require('./src/routes/v2115'));
	router.use('/api/v2116', require('./src/routes/v2116'));
	router.use('/api/v2117', require('./src/routes/v2117'));
	router.use('/api/v2118', require('./src/routes/v2118'));
	router.use('/api/v2119', require('./src/routes/v2119'));
	router.use('/api/v2120', require('./src/routes/v2120'));
	router.use('/api/v2121', require('./src/routes/v2121'));
	router.use('/api/v2122', require('./src/routes/v2122'));
	router.use('/api/v2123', require('./src/routes/v2123'));
	router.use('/api/v2124', require('./src/routes/v2124'));
	router.use('/api/v2125', require('./src/routes/v2125'));
	router.use('/api/v2126', require('./src/routes/v2126'));
	router.use('/api/v2127', require('./src/routes/v2127'));
	router.use('/api/v2128', require('./src/routes/v2128'));
	router.use('/api/v2129', require('./src/routes/v2129'));
	router.use('/api/v2130', require('./src/routes/v2130'));
	router.use('/api/v2131', require('./src/routes/v2131'));
	router.use('/api/v2132', require('./src/routes/v2132'));
	router.use('/api/v2133', require('./src/routes/v2133'));
	router.use('/api/v2134', require('./src/routes/v2134'));
	router.use('/api/v2135', require('./src/routes/v2135'));
	router.use('/api/v2136', require('./src/routes/v2136'));
	router.use('/api/v2137', require('./src/routes/v2137'));
	router.use('/api/v2138', require('./src/routes/v2138'));
	router.use('/api/v2139', require('./src/routes/v2139'));
	router.use('/api/v2140', require('./src/routes/v2140'));
	router.use('/api/v2141', require('./src/routes/v2141'));
	router.use('/api/v2142', require('./src/routes/v2142'));
	router.use('/api/v2143', require('./src/routes/v2143'));
	router.use('/api/v2144', require('./src/routes/v2144'));
	router.use('/api/v2145', require('./src/routes/v2145'));
	router.use('/api/v2146', require('./src/routes/v2146'));
	router.use('/api/v2147', require('./src/routes/v2147'));
	router.use('/api/v2148', require('./src/routes/v2148'));
	router.use('/api/v2149', require('./src/routes/v2149'));
	router.use('/api/v2150', require('./src/routes/v2150'));
	router.use('/api/v2151', require('./src/routes/v2151'));
	router.use('/api/v2152', require('./src/routes/v2152'));
	router.use('/api/v2153', require('./src/routes/v2153'));
	router.use('/api/v2154', require('./src/routes/v2154'));
	router.use('/api/v2155', require('./src/routes/v2155'));
	router.use('/api/v2156', require('./src/routes/v2156'));
	router.use('/api/v2157', require('./src/routes/v2157'));
	router.use('/api/v2158', require('./src/routes/v2158'));
	router.use('/api/v2159', require('./src/routes/v2159'));
	router.use('/api/v2160', require('./src/routes/v2160'));
	router.use('/api/v2161', require('./src/routes/v2161'));
	router.use('/api/v2162', require('./src/routes/v2162'));
	router.use('/api/v2163', require('./src/routes/v2163'));
	router.use('/api/v2164', require('./src/routes/v2164'));
	router.use('/api/v2165', require('./src/routes/v2165'));
	router.use('/api/v2166', require('./src/routes/v2166'));
	router.use('/api/v2167', require('./src/routes/v2167'));
	router.use('/api/v2168', require('./src/routes/v2168'));
	router.use('/api/v2169', require('./src/routes/v2169'));
	router.use('/api/v2170', require('./src/routes/v2170'));
	router.use('/api/v2171', require('./src/routes/v2171'));
	router.use('/api/v2172', require('./src/routes/v2172'));
	router.use('/api/v2173', require('./src/routes/v2173'));
	router.use('/api/v2174', require('./src/routes/v2174'));
	router.use('/api/v2175', require('./src/routes/v2175'));
	router.use('/api/v2176', require('./src/routes/v2176'));
	router.use('/api/v2177', require('./src/routes/v2177'));
	router.use('/api/v2178', require('./src/routes/v2178'));
	router.use('/api/v2179', require('./src/routes/v2179'));
	router.use('/api/v2180', require('./src/routes/v2180'));
	router.use('/api/v2181', require('./src/routes/v2181'));
	router.use('/api/v2182', require('./src/routes/v2182'));
	router.use('/api/v2183', require('./src/routes/v2183'));
	router.use('/api/v2184', require('./src/routes/v2184'));
	router.use('/api/v2185', require('./src/routes/v2185'));
	router.use('/api/v2186', require('./src/routes/v2186'));
	router.use('/api/v2187', require('./src/routes/v2187'));
	router.use('/api/v2188', require('./src/routes/v2188'));
	router.use('/api/v2189', require('./src/routes/v2189'));
	router.use('/api/v2190', require('./src/routes/v2190'));
	router.use('/api/v2191', require('./src/routes/v2191'));
	router.use('/api/v2192', require('./src/routes/v2192'));
	router.use('/api/v2193', require('./src/routes/v2193'));
	router.use('/api/v2194', require('./src/routes/v2194'));
	router.use('/api/v2195', require('./src/routes/v2195'));
	router.use('/api/v2196', require('./src/routes/v2196'));
	router.use('/api/v2197', require('./src/routes/v2197'));
	router.use('/api/v2198', require('./src/routes/v2198'));
	router.use('/api/v2199', require('./src/routes/v2199'));
	router.use('/api/v2200', require('./src/routes/v2200'));
	router.use('/api/v2201', require('./src/routes/v2201'));
	router.use('/api/v2202', require('./src/routes/v2202'));
	router.use('/api/v2203', require('./src/routes/v2203'));
	router.use('/api/v2204', require('./src/routes/v2204'));
	router.use('/api/v2205', require('./src/routes/v2205'));
	router.use('/api/v2206', require('./src/routes/v2206'));
	router.use('/api/v2207', require('./src/routes/v2207'));
	router.use('/api/v2208', require('./src/routes/v2208'));
	router.use('/api/v2209', require('./src/routes/v2209'));
	router.use('/api/v2210', require('./src/routes/v2210'));
	router.use('/api/v2211', require('./src/routes/v2211'));
	router.use('/api/v2212', require('./src/routes/v2212'));
	router.use('/api/v2213', require('./src/routes/v2213'));
	router.use('/api/v2214', require('./src/routes/v2214'));
	router.use('/api/v2215', require('./src/routes/v2215'));
	router.use('/api/v2216', require('./src/routes/v2216'));
	router.use('/api/v2217', require('./src/routes/v2217'));
	router.use('/api/v2218', require('./src/routes/v2218'));
	router.use('/api/v2219', require('./src/routes/v2219'));
	router.use('/api/v2220', require('./src/routes/v2220'));
	router.use('/api/v2221', require('./src/routes/v2221'));
	router.use('/api/v2222', require('./src/routes/v2222'));
	router.use('/api/v2223', require('./src/routes/v2223'));
	router.use('/api/v2224', require('./src/routes/v2224'));
	router.use('/api/v2225', require('./src/routes/v2225'));
	router.use('/api/v2226', require('./src/routes/v2226'));
	router.use('/api/v2227', require('./src/routes/v2227'));
	router.use('/api/v2228', require('./src/routes/v2228'));
	router.use('/api/v2229', require('./src/routes/v2229'));
	router.use('/api/v2230', require('./src/routes/v2230'));
	router.use('/api/v2231', require('./src/routes/v2231'));
	router.use('/api/v2232', require('./src/routes/v2232'));
	router.use('/api/v2233', require('./src/routes/v2233'));
	router.use('/api/v2234', require('./src/routes/v2234'));
	router.use('/api/v2235', require('./src/routes/v2235'));
	router.use('/api/v2236', require('./src/routes/v2236'));
	router.use('/api/v2237', require('./src/routes/v2237'));
	router.use('/api/v2238', require('./src/routes/v2238'));
	router.use('/api/v2239', require('./src/routes/v2239'));
	router.use('/api/v2240', require('./src/routes/v2240'));
	router.use('/api/v2241', require('./src/routes/v2241'));
	router.use('/api/v2242', require('./src/routes/v2242'));
	router.use('/api/v2243', require('./src/routes/v2243'));
	router.use('/api/v2244', require('./src/routes/v2244'));
	router.use('/api/v2245', require('./src/routes/v2245'));
	router.use('/api/v2246', require('./src/routes/v2246'));
	router.use('/api/v2247', require('./src/routes/v2247'));
	router.use('/api/v2248', require('./src/routes/v2248'));
	router.use('/api/v2249', require('./src/routes/v2249'));
	router.use('/api/v2250', require('./src/routes/v2250'));
	router.use('/api/v2251', require('./src/routes/v2251'));
	router.use('/api/v2252', require('./src/routes/v2252'));
	router.use('/api/v2253', require('./src/routes/v2253'));
	router.use('/api/v2254', require('./src/routes/v2254'));
	router.use('/api/v2255', require('./src/routes/v2255'));
	router.use('/api/v2256', require('./src/routes/v2256'));
	router.use('/api/v2257', require('./src/routes/v2257'));
	router.use('/api/v2258', require('./src/routes/v2258'));
	router.use('/api/v2259', require('./src/routes/v2259'));
	router.use('/api/v2260', require('./src/routes/v2260'));
	router.use('/api/v2261', require('./src/routes/v2261'));
	router.use('/api/v2262', require('./src/routes/v2262'));
	router.use('/api/v2263', require('./src/routes/v2263'));
	router.use('/api/v2264', require('./src/routes/v2264'));
	router.use('/api/v2265', require('./src/routes/v2265'));
	router.use('/api/v2266', require('./src/routes/v2266'));
	router.use('/api/v2267', require('./src/routes/v2267'));
	router.use('/api/v2268', require('./src/routes/v2268'));
	router.use('/api/v2269', require('./src/routes/v2269'));
	router.use('/api/v2270', require('./src/routes/v2270'));
	router.use('/api/v2271', require('./src/routes/v2271'));
	router.use('/api/v2272', require('./src/routes/v2272'));
	router.use('/api/v2273', require('./src/routes/v2273'));
	router.use('/api/v2274', require('./src/routes/v2274'));
	router.use('/api/v2275', require('./src/routes/v2275'));
	router.use('/api/v2276', require('./src/routes/v2276'));
	router.use('/api/v2277', require('./src/routes/v2277'));
	router.use('/api/v2278', require('./src/routes/v2278'));
	router.use('/api/v2279', require('./src/routes/v2279'));
	router.use('/api/v2280', require('./src/routes/v2280'));
	router.use('/api/v2281', require('./src/routes/v2281'));
	router.use('/api/v2282', require('./src/routes/v2282'));
	router.use('/api/v2283', require('./src/routes/v2283'));
	router.use('/api/v2284', require('./src/routes/v2284'));
	router.use('/api/v2285', require('./src/routes/v2285'));
	router.use('/api/v2286', require('./src/routes/v2286'));
	router.use('/api/v2287', require('./src/routes/v2287'));
	router.use('/api/v2288', require('./src/routes/v2288'));
	router.use('/api/v2289', require('./src/routes/v2289'));
	router.use('/api/v2290', require('./src/routes/v2290'));
	router.use('/api/v2291', require('./src/routes/v2291'));
	router.use('/api/v2292', require('./src/routes/v2292'));
	router.use('/api/v2293', require('./src/routes/v2293'));
	router.use('/api/v2294', require('./src/routes/v2294'));
	router.use('/api/v2295', require('./src/routes/v2295'));
	router.use('/api/v2296', require('./src/routes/v2296'));
	router.use('/api/v2297', require('./src/routes/v2297'));
	router.use('/api/v2298', require('./src/routes/v2298'));
	router.use('/api/v2299', require('./src/routes/v2299'));
	router.use('/api/v2300', require('./src/routes/v2300'));
	router.use('/api/v2301', require('./src/routes/v2301'));
	router.use('/api/v2302', require('./src/routes/v2302'));
	router.use('/api/v2303', require('./src/routes/v2303'));
	router.use('/api/v2304', require('./src/routes/v2304'));
	router.use('/api/v2305', require('./src/routes/v2305'));
	router.use('/api/v2306', require('./src/routes/v2306'));
	router.use('/api/v2307', require('./src/routes/v2307'));
	router.use('/api/v2308', require('./src/routes/v2308'));
	router.use('/api/v2309', require('./src/routes/v2309'));
	router.use('/api/v2310', require('./src/routes/v2310'));
	router.use('/api/v2311', require('./src/routes/v2311'));
	router.use('/api/v2312', require('./src/routes/v2312'));
	router.use('/api/v2313', require('./src/routes/v2313'));
	router.use('/api/v2314', require('./src/routes/v2314'));
	router.use('/api/v2315', require('./src/routes/v2315'));
	router.use('/api/v2316', require('./src/routes/v2316'));
	router.use('/api/v2317', require('./src/routes/v2317'));
	router.use('/api/v2318', require('./src/routes/v2318'));
	router.use('/api/v2319', require('./src/routes/v2319'));
	router.use('/api/v2320', require('./src/routes/v2320'));
	router.use('/api/v2321', require('./src/routes/v2321'));
	router.use('/api/v2322', require('./src/routes/v2322'));
	router.use('/api/v2323', require('./src/routes/v2323'));
	router.use('/api/v2324', require('./src/routes/v2324'));
	router.use('/api/v2325', require('./src/routes/v2325'));
	router.use('/api/v2326', require('./src/routes/v2326'));
	router.use('/api/v2327', require('./src/routes/v2327'));
	router.use('/api/v2328', require('./src/routes/v2328'));
	router.use('/api/v2329', require('./src/routes/v2329'));
	router.use('/api/v2330', require('./src/routes/v2330'));
	router.use('/api/v2331', require('./src/routes/v2331'));
	router.use('/api/v2332', require('./src/routes/v2332'));
	router.use('/api/v2333', require('./src/routes/v2333'));
	router.use('/api/v2334', require('./src/routes/v2334'));
	router.use('/api/v2335', require('./src/routes/v2335'));
	router.use('/api/v2336', require('./src/routes/v2336'));
	router.use('/api/v2337', require('./src/routes/v2337'));
	router.use('/api/v2338', require('./src/routes/v2338'));
	router.use('/api/v2339', require('./src/routes/v2339'));
	router.use('/api/v2340', require('./src/routes/v2340'));
	router.use('/api/v2341', require('./src/routes/v2341'));
	router.use('/api/v2342', require('./src/routes/v2342'));
	router.use('/api/v2343', require('./src/routes/v2343'));
	router.use('/api/v2344', require('./src/routes/v2344'));
	router.use('/api/v2345', require('./src/routes/v2345'));
	router.use('/api/v2346', require('./src/routes/v2346'));
	router.use('/api/v2347', require('./src/routes/v2347'));
	router.use('/api/v2348', require('./src/routes/v2348'));
	router.use('/api/v2349', require('./src/routes/v2349'));
	router.use('/api/v2350', require('./src/routes/v2350'));
	router.use('/api/v2351', require('./src/routes/v2351'));
	router.use('/api/v2352', require('./src/routes/v2352'));
	router.use('/api/v2353', require('./src/routes/v2353'));
	router.use('/api/v2354', require('./src/routes/v2354'));
	router.use('/api/v2355', require('./src/routes/v2355'));
	router.use('/api/v2356', require('./src/routes/v2356'));
	router.use('/api/v2357', require('./src/routes/v2357'));
	router.use('/api/v2358', require('./src/routes/v2358'));
	router.use('/api/v2359', require('./src/routes/v2359'));
	router.use('/api/v2360', require('./src/routes/v2360'));
	router.use('/api/v2361', require('./src/routes/v2361'));
	router.use('/api/v2362', require('./src/routes/v2362'));
	router.use('/api/v2363', require('./src/routes/v2363'));
	router.use('/api/v2364', require('./src/routes/v2364'));
	router.use('/api/v2365', require('./src/routes/v2365'));
	router.use('/api/v2366', require('./src/routes/v2366'));
	router.use('/api/v2367', require('./src/routes/v2367'));
	router.use('/api/v2368', require('./src/routes/v2368'));
	router.use('/api/v2369', require('./src/routes/v2369'));
	router.use('/api/v2370', require('./src/routes/v2370'));
	router.use('/api/v2371', require('./src/routes/v2371'));
	router.use('/api/v2372', require('./src/routes/v2372'));
	router.use('/api/v2373', require('./src/routes/v2373'));
	router.use('/api/v2374', require('./src/routes/v2374'));
	router.use('/api/v2375', require('./src/routes/v2375'));
	router.use('/api/v2376', require('./src/routes/v2376'));
	router.use('/api/v2377', require('./src/routes/v2377'));
	router.use('/api/v2378', require('./src/routes/v2378'));
	router.use('/api/v2379', require('./src/routes/v2379'));
	router.use('/api/v2380', require('./src/routes/v2380'));
	router.use('/api/v2381', require('./src/routes/v2381'));
	router.use('/api/v2382', require('./src/routes/v2382'));
	router.use('/api/v2383', require('./src/routes/v2383'));
	router.use('/api/v2384', require('./src/routes/v2384'));
	router.use('/api/v2385', require('./src/routes/v2385'));
	router.use('/api/v2386', require('./src/routes/v2386'));
	router.use('/api/v2387', require('./src/routes/v2387'));
	router.use('/api/v2388', require('./src/routes/v2388'));
	router.use('/api/v2389', require('./src/routes/v2389'));
	router.use('/api/v2390', require('./src/routes/v2390'));
	router.use('/api/v2391', require('./src/routes/v2391'));
	router.use('/api/v2392', require('./src/routes/v2392'));
	router.use('/api/v2393', require('./src/routes/v2393'));
	router.use('/api/v2394', require('./src/routes/v2394'));
	router.use('/api/v2395', require('./src/routes/v2395'));
	router.use('/api/v2396', require('./src/routes/v2396'));
	router.use('/api/v2397', require('./src/routes/v2397'));
	router.use('/api/v2398', require('./src/routes/v2398'));
	router.use('/api/v2399', require('./src/routes/v2399'));
	router.use('/api/v2400', require('./src/routes/v2400'));
	router.use('/api/v2401', require('./src/routes/v2401'));
	router.use('/api/v2402', require('./src/routes/v2402'));
	router.use('/api/v2403', require('./src/routes/v2403'));
	router.use('/api/v2404', require('./src/routes/v2404'));
	router.use('/api/v2405', require('./src/routes/v2405'));
	router.use('/api/v2406', require('./src/routes/v2406'));
	router.use('/api/v2407', require('./src/routes/v2407'));
	router.use('/api/v2408', require('./src/routes/v2408'));
	router.use('/api/v2409', require('./src/routes/v2409'));
	router.use('/api/v2410', require('./src/routes/v2410'));
	router.use('/api/v2411', require('./src/routes/v2411'));
	router.use('/api/v2412', require('./src/routes/v2412'));
	router.use('/api/v2413', require('./src/routes/v2413'));
	router.use('/api/v2414', require('./src/routes/v2414'));
	router.use('/api/v2415', require('./src/routes/v2415'));
	router.use('/api/v2416', require('./src/routes/v2416'));
	router.use('/api/v2417', require('./src/routes/v2417'));
	router.use('/api/v2418', require('./src/routes/v2418'));
	router.use('/api/v2419', require('./src/routes/v2419'));
	router.use('/api/v2420', require('./src/routes/v2420'));
	router.use('/api/v2421', require('./src/routes/v2421'));
	router.use('/api/v2422', require('./src/routes/v2422'));
	router.use('/api/v2423', require('./src/routes/v2423'));
	router.use('/api/v2424', require('./src/routes/v2424'));
	router.use('/api/v2425', require('./src/routes/v2425'));
	router.use('/api/v2426', require('./src/routes/v2426'));
	router.use('/api/v2427', require('./src/routes/v2427'));
	router.use('/api/v2428', require('./src/routes/v2428'));
	router.use('/api/v2429', require('./src/routes/v2429'));
	router.use('/api/v2430', require('./src/routes/v2430'));
	router.use('/api/v2431', require('./src/routes/v2431'));
	router.use('/api/v2432', require('./src/routes/v2432'));
	router.use('/api/v2433', require('./src/routes/v2433'));
	router.use('/api/v2434', require('./src/routes/v2434'));
	router.use('/api/v2435', require('./src/routes/v2435'));
	router.use('/api/v2436', require('./src/routes/v2436'));
	router.use('/api/v2437', require('./src/routes/v2437'));
	router.use('/api/v2438', require('./src/routes/v2438'));
	router.use('/api/v2439', require('./src/routes/v2439'));
	router.use('/api/v2440', require('./src/routes/v2440'));
	router.use('/api/v2441', require('./src/routes/v2441'));
	router.use('/api/v2442', require('./src/routes/v2442'));
	router.use('/api/v2443', require('./src/routes/v2443'));
	router.use('/api/v2444', require('./src/routes/v2444'));
	router.use('/api/v2445', require('./src/routes/v2445'));
	router.use('/api/v2446', require('./src/routes/v2446'));
	router.use('/api/v2447', require('./src/routes/v2447'));
	router.use('/api/v2448', require('./src/routes/v2448'));
	router.use('/api/v2449', require('./src/routes/v2449'));
	router.use('/api/v2450', require('./src/routes/v2450'));
	router.use('/api/v2451', require('./src/routes/v2451'));
	router.use('/api/v2452', require('./src/routes/v2452'));
	router.use('/api/v2453', require('./src/routes/v2453'));
	router.use('/api/v2454', require('./src/routes/v2454'));
	router.use('/api/v2455', require('./src/routes/v2455'));
	router.use('/api/v2456', require('./src/routes/v2456'));
	router.use('/api/v2457', require('./src/routes/v2457'));
	router.use('/api/v2458', require('./src/routes/v2458'));
	router.use('/api/v2459', require('./src/routes/v2459'));
	router.use('/api/v2460', require('./src/routes/v2460'));
	router.use('/api/v2461', require('./src/routes/v2461'));
	router.use('/api/v2462', require('./src/routes/v2462'));
	router.use('/api/v2463', require('./src/routes/v2463'));
	router.use('/api/v2464', require('./src/routes/v2464'));
	router.use('/api/v2465', require('./src/routes/v2465'));
	router.use('/api/v2466', require('./src/routes/v2466'));
	router.use('/api/v2467', require('./src/routes/v2467'));
	router.use('/api/v2468', require('./src/routes/v2468'));
	router.use('/api/v2469', require('./src/routes/v2469'));
	router.use('/api/v2470', require('./src/routes/v2470'));
	router.use('/api/v2471', require('./src/routes/v2471'));
	router.use('/api/v2472', require('./src/routes/v2472'));
	router.use('/api/v2473', require('./src/routes/v2473'));
	router.use('/api/v2474', require('./src/routes/v2474'));
	router.use('/api/v2475', require('./src/routes/v2475'));
	router.use('/api/v2476', require('./src/routes/v2476'));
	router.use('/api/v2477', require('./src/routes/v2477'));
	router.use('/api/v2478', require('./src/routes/v2478'));
	router.use('/api/v2479', require('./src/routes/v2479'));
	router.use('/api/v2480', require('./src/routes/v2480'));
	router.use('/api/v2481', require('./src/routes/v2481'));
	router.use('/api/v2482', require('./src/routes/v2482'));
	router.use('/api/v2483', require('./src/routes/v2483'));
	router.use('/api/v2484', require('./src/routes/v2484'));
	router.use('/api/v2485', require('./src/routes/v2485'));
	router.use('/api/v2486', require('./src/routes/v2486'));
	router.use('/api/v2487', require('./src/routes/v2487'));
	router.use('/api/v2488', require('./src/routes/v2488'));
	router.use('/api/v2489', require('./src/routes/v2489'));
	router.use('/api/v2490', require('./src/routes/v2490'));
	router.use('/api/v2491', require('./src/routes/v2491'));
	router.use('/api/v2492', require('./src/routes/v2492'));
	router.use('/api/v2493', require('./src/routes/v2493'));
	router.use('/api/v2494', require('./src/routes/v2494'));
	router.use('/api/v2495', require('./src/routes/v2495'));
	router.use('/api/v2496', require('./src/routes/v2496'));
	router.use('/api/v2497', require('./src/routes/v2497'));
	router.use('/api/v2498', require('./src/routes/v2498'));
	router.use('/api/v2499', require('./src/routes/v2499'));
	router.use('/api/v2500', require('./src/routes/v2500'));
	router.use('/api/v2501', require('./src/routes/v2501'));
	router.use('/api/v2502', require('./src/routes/v2502'));
	router.use('/api/v2503', require('./src/routes/v2503'));
	router.use('/api/v2504', require('./src/routes/v2504'));
	router.use('/api/v2505', require('./src/routes/v2505'));
	router.use('/api/v2506', require('./src/routes/v2506'));
	router.use('/api/v2507', require('./src/routes/v2507'));
	router.use('/api/v2508', require('./src/routes/v2508'));
	router.use('/api/v2509', require('./src/routes/v2509'));
	router.use('/api/v2510', require('./src/routes/v2510'));
	router.use('/api/v2511', require('./src/routes/v2511'));
	router.use('/api/v2512', require('./src/routes/v2512'));
	router.use('/api/v2513', require('./src/routes/v2513'));
	router.use('/api/v2514', require('./src/routes/v2514'));
	router.use('/api/v2515', require('./src/routes/v2515'));
	router.use('/api/v2516', require('./src/routes/v2516'));
	router.use('/api/v2517', require('./src/routes/v2517'));
	router.use('/api/v2518', require('./src/routes/v2518'));
	router.use('/api/v2519', require('./src/routes/v2519'));
	router.use('/api/v2520', require('./src/routes/v2520'));
	router.use('/api/v2521', require('./src/routes/v2521'));
	router.use('/api/v2522', require('./src/routes/v2522'));
	router.use('/api/v2523', require('./src/routes/v2523'));
	router.use('/api/v2524', require('./src/routes/v2524'));
	router.use('/api/v2525', require('./src/routes/v2525'));
	router.use('/api/v2526', require('./src/routes/v2526'));
	router.use('/api/v2527', require('./src/routes/v2527'));
	router.use('/api/v2528', require('./src/routes/v2528'));
	router.use('/api/v2529', require('./src/routes/v2529'));
	router.use('/api/v2530', require('./src/routes/v2530'));
	router.use('/api/v2531', require('./src/routes/v2531'));
	router.use('/api/v2532', require('./src/routes/v2532'));
	router.use('/api/v2533', require('./src/routes/v2533'));
	router.use('/api/v2534', require('./src/routes/v2534'));
	router.use('/api/v2535', require('./src/routes/v2535'));
	router.use('/api/v2536', require('./src/routes/v2536'));
	router.use('/api/v2537', require('./src/routes/v2537'));
	router.use('/api/v2538', require('./src/routes/v2538'));
	router.use('/api/v2539', require('./src/routes/v2539'));
	router.use('/api/v2540', require('./src/routes/v2540'));
	router.use('/api/v2541', require('./src/routes/v2541'));
	router.use('/api/v2542', require('./src/routes/v2542'));
	router.use('/api/v2543', require('./src/routes/v2543'));
	router.use('/api/v2544', require('./src/routes/v2544'));
	router.use('/api/v2545', require('./src/routes/v2545'));
	router.use('/api/v2546', require('./src/routes/v2546'));
	router.use('/api/v2547', require('./src/routes/v2547'));
	router.use('/api/v2548', require('./src/routes/v2548'));
	router.use('/api/v2549', require('./src/routes/v2549'));
	router.use('/api/v2550', require('./src/routes/v2550'));
	router.use('/api/v2551', require('./src/routes/v2551'));
	router.use('/api/v2552', require('./src/routes/v2552'));
	router.use('/api/v2553', require('./src/routes/v2553'));
	router.use('/api/v2554', require('./src/routes/v2554'));
	router.use('/api/v2555', require('./src/routes/v2555'));
	router.use('/api/v2556', require('./src/routes/v2556'));
	router.use('/api/v2557', require('./src/routes/v2557'));
	router.use('/api/v2558', require('./src/routes/v2558'));
	router.use('/api/v2559', require('./src/routes/v2559'));
	router.use('/api/v2560', require('./src/routes/v2560'));
	router.use('/api/v2561', require('./src/routes/v2561'));
	router.use('/api/v2562', require('./src/routes/v2562'));
	router.use('/api/v2563', require('./src/routes/v2563'));
	router.use('/api/v2564', require('./src/routes/v2564'));
	router.use('/api/v2565', require('./src/routes/v2565'));
	router.use('/api/v2566', require('./src/routes/v2566'));
	router.use('/api/v2567', require('./src/routes/v2567'));
	router.use('/api/v2568', require('./src/routes/v2568'));
	router.use('/api/v2569', require('./src/routes/v2569'));
	router.use('/api/v2570', require('./src/routes/v2570'));
	router.use('/api/v2571', require('./src/routes/v2571'));
	router.use('/api/v2572', require('./src/routes/v2572'));
	router.use('/api/v2573', require('./src/routes/v2573'));
	router.use('/api/v2574', require('./src/routes/v2574'));
	router.use('/api/v2575', require('./src/routes/v2575'));
	router.use('/api/v2576', require('./src/routes/v2576'));
	router.use('/api/v2577', require('./src/routes/v2577'));
	router.use('/api/v2578', require('./src/routes/v2578'));
	router.use('/api/v2579', require('./src/routes/v2579'));
	router.use('/api/v2580', require('./src/routes/v2580'));
	router.use('/api/v2581', require('./src/routes/v2581'));
	router.use('/api/v2582', require('./src/routes/v2582'));
	router.use('/api/v2583', require('./src/routes/v2583'));
	router.use('/api/v2584', require('./src/routes/v2584'));
	router.use('/api/v2585', require('./src/routes/v2585'));
	router.use('/api/v2586', require('./src/routes/v2586'));
	router.use('/api/v2587', require('./src/routes/v2587'));
	router.use('/api/v2588', require('./src/routes/v2588'));
	router.use('/api/v2589', require('./src/routes/v2589'));
	router.use('/api/v2590', require('./src/routes/v2590'));
	router.use('/api/v2591', require('./src/routes/v2591'));
	router.use('/api/v2592', require('./src/routes/v2592'));
	router.use('/api/v2593', require('./src/routes/v2593'));
	router.use('/api/v2594', require('./src/routes/v2594'));
	router.use('/api/v2595', require('./src/routes/v2595'));
	router.use('/api/v2596', require('./src/routes/v2596'));
	router.use('/api/v2597', require('./src/routes/v2597'));
	router.use('/api/v2598', require('./src/routes/v2598'));
	router.use('/api/v2599', require('./src/routes/v2599'));
	router.use('/api/v2600', require('./src/routes/v2600'));
	router.use('/api/v2601', require('./src/routes/v2601'));
	router.use('/api/v2602', require('./src/routes/v2602'));
	router.use('/api/v2603', require('./src/routes/v2603'));
	router.use('/api/v2604', require('./src/routes/v2604'));
	router.use('/api/v2605', require('./src/routes/v2605'));
	router.use('/api/v2606', require('./src/routes/v2606'));
	router.use('/api/v2607', require('./src/routes/v2607'));
	router.use('/api/v2608', require('./src/routes/v2608'));
	router.use('/api/v2609', require('./src/routes/v2609'));
	router.use('/api/v2610', require('./src/routes/v2610'));
	router.use('/api/v2611', require('./src/routes/v2611'));
	router.use('/api/v2612', require('./src/routes/v2612'));
	router.use('/api/v2613', require('./src/routes/v2613'));
	router.use('/api/v2614', require('./src/routes/v2614'));
	router.use('/api/v2615', require('./src/routes/v2615'));
	router.use('/api/v2616', require('./src/routes/v2616'));
	router.use('/api/v2617', require('./src/routes/v2617'));
	router.use('/api/v2618', require('./src/routes/v2618'));
	router.use('/api/v2619', require('./src/routes/v2619'));
	router.use('/api/v2620', require('./src/routes/v2620'));
	router.use('/api/v2621', require('./src/routes/v2621'));
	router.use('/api/v2622', require('./src/routes/v2622'));
	router.use('/api/v2623', require('./src/routes/v2623'));
	router.use('/api/v2624', require('./src/routes/v2624'));
	router.use('/api/v2625', require('./src/routes/v2625'));
	router.use('/api/v2626', require('./src/routes/v2626'));
	router.use('/api/v2627', require('./src/routes/v2627'));
	router.use('/api/v2628', require('./src/routes/v2628'));
	router.use('/api/v2629', require('./src/routes/v2629'));
	router.use('/api/v2630', require('./src/routes/v2630'));
	router.use('/api/v2631', require('./src/routes/v2631'));
	router.use('/api/v2632', require('./src/routes/v2632'));
	router.use('/api/v2633', require('./src/routes/v2633'));
	router.use('/api/v2634', require('./src/routes/v2634'));
	router.use('/api/v2635', require('./src/routes/v2635'));
	router.use('/api/v2636', require('./src/routes/v2636'));
	router.use('/api/v2637', require('./src/routes/v2637'));
	router.use('/api/v2638', require('./src/routes/v2638'));
	router.use('/api/v2639', require('./src/routes/v2639'));
	router.use('/api/v2640', require('./src/routes/v2640'));
	router.use('/api/v2641', require('./src/routes/v2641'));
	router.use('/api/v2642', require('./src/routes/v2642'));
	router.use('/api/v2643', require('./src/routes/v2643'));
	router.use('/api/v2644', require('./src/routes/v2644'));
	router.use('/api/v2645', require('./src/routes/v2645'));
	router.use('/api/v2646', require('./src/routes/v2646'));
	router.use('/api/v2647', require('./src/routes/v2647'));
	router.use('/api/v2648', require('./src/routes/v2648'));
	router.use('/api/v2649', require('./src/routes/v2649'));
	router.use('/api/v2650', require('./src/routes/v2650'));
	router.use('/api/v2651', require('./src/routes/v2651'));
	router.use('/api/v2652', require('./src/routes/v2652'));
	router.use('/api/v2653', require('./src/routes/v2653'));
	router.use('/api/v2654', require('./src/routes/v2654'));
	router.use('/api/v2655', require('./src/routes/v2655'));
	router.use('/api/v2656', require('./src/routes/v2656'));
	router.use('/api/v2657', require('./src/routes/v2657'));
	router.use('/api/v2658', require('./src/routes/v2658'));
	router.use('/api/v2659', require('./src/routes/v2659'));
	router.use('/api/v2660', require('./src/routes/v2660'));
	router.use('/api/v2661', require('./src/routes/v2661'));
	router.use('/api/v2662', require('./src/routes/v2662'));
	router.use('/api/v2663', require('./src/routes/v2663'));
	router.use('/api/v2664', require('./src/routes/v2664'));
	router.use('/api/v2665', require('./src/routes/v2665'));
	router.use('/api/v2666', require('./src/routes/v2666'));
	router.use('/api/v2667', require('./src/routes/v2667'));
	router.use('/api/v2668', require('./src/routes/v2668'));
	router.use('/api/v2669', require('./src/routes/v2669'));
	router.use('/api/v2670', require('./src/routes/v2670'));
	router.use('/api/v2671', require('./src/routes/v2671'));
	router.use('/api/v2672', require('./src/routes/v2672'));
	router.use('/api/v2673', require('./src/routes/v2673'));
	router.use('/api/v2674', require('./src/routes/v2674'));
	router.use('/api/v2675', require('./src/routes/v2675'));
	router.use('/api/v2676', require('./src/routes/v2676'));
	router.use('/api/v2677', require('./src/routes/v2677'));
	router.use('/api/v2678', require('./src/routes/v2678'));
	router.use('/api/v2679', require('./src/routes/v2679'));
	router.use('/api/v2680', require('./src/routes/v2680'));
	router.use('/api/v2681', require('./src/routes/v2681'));
	router.use('/api/v2682', require('./src/routes/v2682'));
	router.use('/api/v2683', require('./src/routes/v2683'));
	router.use('/api/v2684', require('./src/routes/v2684'));
	router.use('/api/v2685', require('./src/routes/v2685'));
	router.use('/api/v2686', require('./src/routes/v2686'));
	router.use('/api/v2687', require('./src/routes/v2687'));
	router.use('/api/v2688', require('./src/routes/v2688'));
	router.use('/api/v2689', require('./src/routes/v2689'));
	router.use('/api/v2690', require('./src/routes/v2690'));
	router.use('/api/v2691', require('./src/routes/v2691'));
	router.use('/api/v2692', require('./src/routes/v2692'));
	router.use('/api/v2693', require('./src/routes/v2693'));
	router.use('/api/v2694', require('./src/routes/v2694'));
	router.use('/api/v2695', require('./src/routes/v2695'));
	router.use('/api/v2696', require('./src/routes/v2696'));
	router.use('/api/v2697', require('./src/routes/v2697'));
	router.use('/api/v2698', require('./src/routes/v2698'));
	router.use('/api/v2699', require('./src/routes/v2699'));
	router.use('/api/v2700', require('./src/routes/v2700'));
	router.use('/api/v2701', require('./src/routes/v2701'));
	router.use('/api/v2702', require('./src/routes/v2702'));
	router.use('/api/v2703', require('./src/routes/v2703'));
	router.use('/api/v2704', require('./src/routes/v2704'));
	router.use('/api/v2705', require('./src/routes/v2705'));
	router.use('/api/v2706', require('./src/routes/v2706'));
	router.use('/api/v2707', require('./src/routes/v2707'));
	router.use('/api/v2708', require('./src/routes/v2708'));
	router.use('/api/v2709', require('./src/routes/v2709'));
	router.use('/api/v2710', require('./src/routes/v2710'));
	router.use('/api/v2711', require('./src/routes/v2711'));
	router.use('/api/v2712', require('./src/routes/v2712'));
	router.use('/api/v2713', require('./src/routes/v2713'));
	router.use('/api/v2714', require('./src/routes/v2714'));
	router.use('/api/v2715', require('./src/routes/v2715'));
	router.use('/api/v2716', require('./src/routes/v2716'));
	router.use('/api/v2717', require('./src/routes/v2717'));
	router.use('/api/v2718', require('./src/routes/v2718'));
	router.use('/api/v2719', require('./src/routes/v2719'));
	router.use('/api/v2720', require('./src/routes/v2720'));
	router.use('/api/v2721', require('./src/routes/v2721'));
	router.use('/api/v2722', require('./src/routes/v2722'));
	router.use('/api/v2723', require('./src/routes/v2723'));
	router.use('/api/v2724', require('./src/routes/v2724'));
	router.use('/api/v2725', require('./src/routes/v2725'));
	router.use('/api/v2726', require('./src/routes/v2726'));
	router.use('/api/v2727', require('./src/routes/v2727'));
	router.use('/api/v2728', require('./src/routes/v2728'));
	router.use('/api/v2729', require('./src/routes/v2729'));
	router.use('/api/v2730', require('./src/routes/v2730'));
	router.use('/api/v2731', require('./src/routes/v2731'));
	router.use('/api/v2732', require('./src/routes/v2732'));
	router.use('/api/v2733', require('./src/routes/v2733'));
	router.use('/api/v2734', require('./src/routes/v2734'));
	router.use('/api/v2735', require('./src/routes/v2735'));
	router.use('/api/v2736', require('./src/routes/v2736'));
	router.use('/api/v2737', require('./src/routes/v2737'));
	router.use('/api/v2738', require('./src/routes/v2738'));
	router.use('/api/v2739', require('./src/routes/v2739'));
	router.use('/api/v2740', require('./src/routes/v2740'));
	router.use('/api/v2741', require('./src/routes/v2741'));
	router.use('/api/v2742', require('./src/routes/v2742'));
	router.use('/api/v2743', require('./src/routes/v2743'));
	router.use('/api/v2744', require('./src/routes/v2744'));
	router.use('/api/v2745', require('./src/routes/v2745'));
	router.use('/api/v2746', require('./src/routes/v2746'));
	router.use('/api/v2747', require('./src/routes/v2747'));
	router.use('/api/v2748', require('./src/routes/v2748'));
	router.use('/api/v2749', require('./src/routes/v2749'));
	router.use('/api/v2750', require('./src/routes/v2750'));
	router.use('/api/v2751', require('./src/routes/v2751'));
	router.use('/api/v2752', require('./src/routes/v2752'));
	router.use('/api/v2753', require('./src/routes/v2753'));
	router.use('/api/v2754', require('./src/routes/v2754'));
	router.use('/api/v2755', require('./src/routes/v2755'));
	router.use('/api/v2756', require('./src/routes/v2756'));
	router.use('/api/v2757', require('./src/routes/v2757'));
	router.use('/api/v2758', require('./src/routes/v2758'));
	router.use('/api/v2759', require('./src/routes/v2759'));
	router.use('/api/v2760', require('./src/routes/v2760'));
	router.use('/api/v2761', require('./src/routes/v2761'));
	router.use('/api/v2762', require('./src/routes/v2762'));
	router.use('/api/v2763', require('./src/routes/v2763'));
	router.use('/api/v2764', require('./src/routes/v2764'));
	router.use('/api/v2765', require('./src/routes/v2765'));
	router.use('/api/v2766', require('./src/routes/v2766'));
	router.use('/api/v2767', require('./src/routes/v2767'));
	router.use('/api/v2768', require('./src/routes/v2768'));
	router.use('/api/v2769', require('./src/routes/v2769'));
	router.use('/api/v2770', require('./src/routes/v2770'));
	router.use('/api/v2771', require('./src/routes/v2771'));
	router.use('/api/v2772', require('./src/routes/v2772'));
	router.use('/api/v2773', require('./src/routes/v2773'));
	router.use('/api/v2774', require('./src/routes/v2774'));
	router.use('/api/v2775', require('./src/routes/v2775'));
	router.use('/api/v2776', require('./src/routes/v2776'));
	router.use('/api/v2777', require('./src/routes/v2777'));
	router.use('/api/v2778', require('./src/routes/v2778'));
	router.use('/api/v2779', require('./src/routes/v2779'));
	router.use('/api/v2780', require('./src/routes/v2780'));
	router.use('/api/v2781', require('./src/routes/v2781'));
	router.use('/api/v2782', require('./src/routes/v2782'));
	router.use('/api/v2783', require('./src/routes/v2783'));
	router.use('/api/v2784', require('./src/routes/v2784'));
	router.use('/api/v2785', require('./src/routes/v2785'));
	router.use('/api/v2786', require('./src/routes/v2786'));
	router.use('/api/v2787', require('./src/routes/v2787'));
	router.use('/api/v2788', require('./src/routes/v2788'));
	router.use('/api/v2789', require('./src/routes/v2789'));
	router.use('/api/v2790', require('./src/routes/v2790'));
	router.use('/api/v2791', require('./src/routes/v2791'));
	router.use('/api/v2792', require('./src/routes/v2792'));
	router.use('/api/v2793', require('./src/routes/v2793'));
	router.use('/api/v2794', require('./src/routes/v2794'));
	router.use('/api/v2795', require('./src/routes/v2795'));
	router.use('/api/v2796', require('./src/routes/v2796'));
	router.use('/api/v2797', require('./src/routes/v2797'));
	router.use('/api/v2798', require('./src/routes/v2798'));
	router.use('/api/v2799', require('./src/routes/v2799'));
	router.use('/api/v2800', require('./src/routes/v2800'));
	router.use('/api/v2801', require('./src/routes/v2801'));
	router.use('/api/v2802', require('./src/routes/v2802'));
	router.use('/api/v2803', require('./src/routes/v2803'));
	router.use('/api/v2804', require('./src/routes/v2804'));
	router.use('/api/v2805', require('./src/routes/v2805'));
	router.use('/api/v2806', require('./src/routes/v2806'));
	router.use('/api/v2807', require('./src/routes/v2807'));
	router.use('/api/v2808', require('./src/routes/v2808'));
	router.use('/api/v2809', require('./src/routes/v2809'));
	router.use('/api/v2810', require('./src/routes/v2810'));
	router.use('/api/v2811', require('./src/routes/v2811'));
	router.use('/api/v2812', require('./src/routes/v2812'));
	router.use('/api/v2813', require('./src/routes/v2813'));
	router.use('/api/v2814', require('./src/routes/v2814'));
	router.use('/api/v2815', require('./src/routes/v2815'));
	router.use('/api/v2816', require('./src/routes/v2816'));
	router.use('/api/v2817', require('./src/routes/v2817'));
	router.use('/api/v2818', require('./src/routes/v2818'));
	router.use('/api/v2819', require('./src/routes/v2819'));
	router.use('/api/v2820', require('./src/routes/v2820'));
	router.use('/api/v2821', require('./src/routes/v2821'));
	router.use('/api/v2822', require('./src/routes/v2822'));
	router.use('/api/v2823', require('./src/routes/v2823'));
	router.use('/api/v2824', require('./src/routes/v2824'));
	router.use('/api/v2825', require('./src/routes/v2825'));
	router.use('/api/v2826', require('./src/routes/v2826'));
	router.use('/api/v2827', require('./src/routes/v2827'));
	router.use('/api/v2828', require('./src/routes/v2828'));
	router.use('/api/v2829', require('./src/routes/v2829'));
	router.use('/api/v2830', require('./src/routes/v2830'));
	router.use('/api/v2831', require('./src/routes/v2831'));
	router.use('/api/v2832', require('./src/routes/v2832'));
	router.use('/api/v2833', require('./src/routes/v2833'));
	router.use('/api/v2834', require('./src/routes/v2834'));
	router.use('/api/v2835', require('./src/routes/v2835'));
	router.use('/api/v2836', require('./src/routes/v2836'));
	router.use('/api/v2837', require('./src/routes/v2837'));
	router.use('/api/v2838', require('./src/routes/v2838'));
	router.use('/api/v2839', require('./src/routes/v2839'));
	router.use('/api/v2840', require('./src/routes/v2840'));
	router.use('/api/v2841', require('./src/routes/v2841'));
	router.use('/api/v2842', require('./src/routes/v2842'));
	router.use('/api/v2843', require('./src/routes/v2843'));
	router.use('/api/v2844', require('./src/routes/v2844'));
	router.use('/api/v2845', require('./src/routes/v2845'));
	router.use('/api/v2846', require('./src/routes/v2846'));
	router.use('/api/v2847', require('./src/routes/v2847'));
	router.use('/api/v2848', require('./src/routes/v2848'));
	router.use('/api/v2849', require('./src/routes/v2849'));
	router.use('/api/v2850', require('./src/routes/v2850'));
	router.use('/api/v2851', require('./src/routes/v2851'));
	router.use('/api/v2852', require('./src/routes/v2852'));
	router.use('/api/v2853', require('./src/routes/v2853'));
	router.use('/api/v2854', require('./src/routes/v2854'));
	router.use('/api/v2855', require('./src/routes/v2855'));
	router.use('/api/v2856', require('./src/routes/v2856'));
	router.use('/api/v2857', require('./src/routes/v2857'));
	router.use('/api/v2858', require('./src/routes/v2858'));
	router.use('/api/v2859', require('./src/routes/v2859'));
	router.use('/api/v2860', require('./src/routes/v2860'));
	router.use('/api/v2861', require('./src/routes/v2861'));
	router.use('/api/v2862', require('./src/routes/v2862'));
	router.use('/api/v2863', require('./src/routes/v2863'));
	router.use('/api/v2864', require('./src/routes/v2864'));
	router.use('/api/v2865', require('./src/routes/v2865'));
	router.use('/api/v2866', require('./src/routes/v2866'));
	router.use('/api/v2867', require('./src/routes/v2867'));
	router.use('/api/v2868', require('./src/routes/v2868'));
	router.use('/api/v2869', require('./src/routes/v2869'));
	router.use('/api/v2870', require('./src/routes/v2870'));
	router.use('/api/v2871', require('./src/routes/v2871'));
	router.use('/api/v2872', require('./src/routes/v2872'));
	router.use('/api/v2873', require('./src/routes/v2873'));
	router.use('/api/v2874', require('./src/routes/v2874'));
	router.use('/api/v2875', require('./src/routes/v2875'));
	router.use('/api/v2876', require('./src/routes/v2876'));
	router.use('/api/v2877', require('./src/routes/v2877'));
	router.use('/api/v2878', require('./src/routes/v2878'));
	router.use('/api/v2879', require('./src/routes/v2879'));
	router.use('/api/v2880', require('./src/routes/v2880'));
	router.use('/api/v2881', require('./src/routes/v2881'));
	router.use('/api/v2882', require('./src/routes/v2882'));
	router.use('/api/v2883', require('./src/routes/v2883'));
	router.use('/api/v2884', require('./src/routes/v2884'));
	router.use('/api/v2885', require('./src/routes/v2885'));
	router.use('/api/v2886', require('./src/routes/v2886'));
	router.use('/api/v2887', require('./src/routes/v2887'));
	router.use('/api/v2888', require('./src/routes/v2888'));
	router.use('/api/v2889', require('./src/routes/v2889'));
	router.use('/api/v2890', require('./src/routes/v2890'));
	router.use('/api/v2891', require('./src/routes/v2891'));
	router.use('/api/v2892', require('./src/routes/v2892'));
	router.use('/api/v2893', require('./src/routes/v2893'));
	router.use('/api/v2894', require('./src/routes/v2894'));
	router.use('/api/v2895', require('./src/routes/v2895'));
	router.use('/api/v2896', require('./src/routes/v2896'));
	router.use('/api/v2897', require('./src/routes/v2897'));
	router.use('/api/v2898', require('./src/routes/v2898'));
	router.use('/api/v2899', require('./src/routes/v2899'));
	router.use('/api/v2900', require('./src/routes/v2900'));
	router.use('/api/v2901', require('./src/routes/v2901'));
	router.use('/api/v2902', require('./src/routes/v2902'));
	router.use('/api/v2903', require('./src/routes/v2903'));
	router.use('/api/v2904', require('./src/routes/v2904'));
	router.use('/api/v2905', require('./src/routes/v2905'));
	router.use('/api/v2906', require('./src/routes/v2906'));
	router.use('/api/v2907', require('./src/routes/v2907'));
	router.use('/api/v2908', require('./src/routes/v2908'));
	router.use('/api/v2909', require('./src/routes/v2909'));
	router.use('/api/v2910', require('./src/routes/v2910'));
	router.use('/api/v2911', require('./src/routes/v2911'));
	router.use('/api/v2912', require('./src/routes/v2912'));
	router.use('/api/v2913', require('./src/routes/v2913'));
	router.use('/api/v2914', require('./src/routes/v2914'));
	router.use('/api/v2915', require('./src/routes/v2915'));
	router.use('/api/v2916', require('./src/routes/v2916'));
	router.use('/api/v2917', require('./src/routes/v2917'));
	router.use('/api/v2918', require('./src/routes/v2918'));
	router.use('/api/v2919', require('./src/routes/v2919'));
	router.use('/api/v2920', require('./src/routes/v2920'));
	router.use('/api/v2921', require('./src/routes/v2921'));
	router.use('/api/v2922', require('./src/routes/v2922'));
	router.use('/api/v2923', require('./src/routes/v2923'));
	router.use('/api/v2924', require('./src/routes/v2924'));
	router.use('/api/v2925', require('./src/routes/v2925'));
	router.use('/api/v2926', require('./src/routes/v2926'));
	router.use('/api/v2927', require('./src/routes/v2927'));
	router.use('/api/v2928', require('./src/routes/v2928'));
	router.use('/api/v2929', require('./src/routes/v2929'));
	router.use('/api/v2930', require('./src/routes/v2930'));
	router.use('/api/v2931', require('./src/routes/v2931'));
	router.use('/api/v2932', require('./src/routes/v2932'));
	router.use('/api/v2933', require('./src/routes/v2933'));
	router.use('/api/v2934', require('./src/routes/v2934'));
	router.use('/api/v2935', require('./src/routes/v2935'));
	router.use('/api/v2936', require('./src/routes/v2936'));
	router.use('/api/v2937', require('./src/routes/v2937'));
	router.use('/api/v2938', require('./src/routes/v2938'));
	router.use('/api/v2939', require('./src/routes/v2939'));
	router.use('/api/v2940', require('./src/routes/v2940'));
	router.use('/api/v2941', require('./src/routes/v2941'));
	router.use('/api/v2942', require('./src/routes/v2942'));
	router.use('/api/v2943', require('./src/routes/v2943'));
	router.use('/api/v2944', require('./src/routes/v2944'));
	router.use('/api/v2945', require('./src/routes/v2945'));
	router.use('/api/v2946', require('./src/routes/v2946'));
	router.use('/api/v2947', require('./src/routes/v2947'));
	router.use('/api/v2948', require('./src/routes/v2948'));
	router.use('/api/v2949', require('./src/routes/v2949'));
	router.use('/api/v2950', require('./src/routes/v2950'));
	router.use('/api/v2951', require('./src/routes/v2951'));
	router.use('/api/v2952', require('./src/routes/v2952'));
	router.use('/api/v2953', require('./src/routes/v2953'));
	router.use('/api/v2954', require('./src/routes/v2954'));
	router.use('/api/v2955', require('./src/routes/v2955'));
	router.use('/api/v2956', require('./src/routes/v2956'));
	router.use('/api/v2957', require('./src/routes/v2957'));
	router.use('/api/v2958', require('./src/routes/v2958'));
	router.use('/api/v2959', require('./src/routes/v2959'));
	router.use('/api/v2960', require('./src/routes/v2960'));
	router.use('/api/v2961', require('./src/routes/v2961'));
	router.use('/api/v2962', require('./src/routes/v2962'));
	router.use('/api/v2963', require('./src/routes/v2963'));
	router.use('/api/v2964', require('./src/routes/v2964'));
	router.use('/api/v2965', require('./src/routes/v2965'));
	router.use('/api/v2966', require('./src/routes/v2966'));
	router.use('/api/v2967', require('./src/routes/v2967'));
	router.use('/api/v2968', require('./src/routes/v2968'));
	router.use('/api/v2969', require('./src/routes/v2969'));
	router.use('/api/v2970', require('./src/routes/v2970'));
	router.use('/api/v2971', require('./src/routes/v2971'));
	router.use('/api/v2972', require('./src/routes/v2972'));
	router.use('/api/v2973', require('./src/routes/v2973'));
	router.use('/api/v2974', require('./src/routes/v2974'));
	router.use('/api/v2975', require('./src/routes/v2975'));
	router.use('/api/v2976', require('./src/routes/v2976'));
	router.use('/api/v2977', require('./src/routes/v2977'));
	router.use('/api/v2978', require('./src/routes/v2978'));
	router.use('/api/v2979', require('./src/routes/v2979'));
	router.use('/api/v2980', require('./src/routes/v2980'));
	router.use('/api/v2981', require('./src/routes/v2981'));
	router.use('/api/v2982', require('./src/routes/v2982'));
	router.use('/api/v2983', require('./src/routes/v2983'));
	router.use('/api/v2984', require('./src/routes/v2984'));
	router.use('/api/v2985', require('./src/routes/v2985'));
	router.use('/api/v2986', require('./src/routes/v2986'));
	router.use('/api/v2987', require('./src/routes/v2987'));
	router.use('/api/v2988', require('./src/routes/v2988'));
	router.use('/api/v2989', require('./src/routes/v2989'));
	router.use('/api/v2990', require('./src/routes/v2990'));
	router.use('/api/v2991', require('./src/routes/v2991'));
	router.use('/api/v2992', require('./src/routes/v2992'));
	router.use('/api/v2993', require('./src/routes/v2993'));
	router.use('/api/v2994', require('./src/routes/v2994'));
	router.use('/api/v2995', require('./src/routes/v2995'));
	router.use('/api/v2996', require('./src/routes/v2996'));
	router.use('/api/v2997', require('./src/routes/v2997'));
	router.use('/api/v2998', require('./src/routes/v2998'));
	router.use('/api/v2999', require('./src/routes/v2999'));
	router.use('/api/v3000', require('./src/routes/v3000'));
	router.use('/api/v3001', require('./src/routes/v3001'));
	router.use('/api/v3002', require('./src/routes/v3002'));
	router.use('/api/v3003', require('./src/routes/v3003'));
	router.use('/api/v3004', require('./src/routes/v3004'));
	router.use('/api/v3005', require('./src/routes/v3005'));
	router.use('/api/v3006', require('./src/routes/v3006'));
	router.use('/api/v3007', require('./src/routes/v3007'));
	router.use('/api/v3008', require('./src/routes/v3008'));
	router.use('/api/v3009', require('./src/routes/v3009'));
	router.use('/api/v3010', require('./src/routes/v3010'));
	router.use('/api/v3011', require('./src/routes/v3011'));
	router.use('/api/v3012', require('./src/routes/v3012'));
	router.use('/api/v3013', require('./src/routes/v3013'));
	router.use('/api/v3014', require('./src/routes/v3014'));
	router.use('/api/v3015', require('./src/routes/v3015'));
	router.use('/api/v3016', require('./src/routes/v3016'));
	router.use('/api/v3017', require('./src/routes/v3017'));
	router.use('/api/v3018', require('./src/routes/v3018'));
	router.use('/api/v3019', require('./src/routes/v3019'));
	router.use('/api/v3020', require('./src/routes/v3020'));
	router.use('/api/v3021', require('./src/routes/v3021'));
	router.use('/api/v3022', require('./src/routes/v3022'));
	router.use('/api/v3023', require('./src/routes/v3023'));
	router.use('/api/v3024', require('./src/routes/v3024'));
	router.use('/api/v3025', require('./src/routes/v3025'));
	router.use('/api/v3026', require('./src/routes/v3026'));
	router.use('/api/v3027', require('./src/routes/v3027'));
	router.use('/api/v3028', require('./src/routes/v3028'));
	router.use('/api/v3029', require('./src/routes/v3029'));
	router.use('/api/v3030', require('./src/routes/v3030'));
	router.use('/api/v3031', require('./src/routes/v3031'));
	router.use('/api/v3032', require('./src/routes/v3032'));
	router.use('/api/v3033', require('./src/routes/v3033'));
	router.use('/api/v3034', require('./src/routes/v3034'));
	router.use('/api/v3035', require('./src/routes/v3035'));
	router.use('/api/v3036', require('./src/routes/v3036'));
	router.use('/api/v3037', require('./src/routes/v3037'));
	router.use('/api/v3038', require('./src/routes/v3038'));
	router.use('/api/v3039', require('./src/routes/v3039'));
	router.use('/api/v3040', require('./src/routes/v3040'));
	router.use('/api/v3041', require('./src/routes/v3041'));
	router.use('/api/v3042', require('./src/routes/v3042'));
	router.use('/api/v3043', require('./src/routes/v3043'));
	router.use('/api/v3044', require('./src/routes/v3044'));
	router.use('/api/v3045', require('./src/routes/v3045'));
	router.use('/api/v3046', require('./src/routes/v3046'));
	router.use('/api/v3047', require('./src/routes/v3047'));
	router.use('/api/v3048', require('./src/routes/v3048'));
	router.use('/api/v3049', require('./src/routes/v3049'));
	router.use('/api/v3050', require('./src/routes/v3050'));
	router.use('/api/v3051', require('./src/routes/v3051'));
	router.use('/api/v3052', require('./src/routes/v3052'));
	router.use('/api/v3053', require('./src/routes/v3053'));
	router.use('/api/v3054', require('./src/routes/v3054'));
	router.use('/api/v3055', require('./src/routes/v3055'));
	router.use('/api/v3056', require('./src/routes/v3056'));
	router.use('/api/v3057', require('./src/routes/v3057'));
	router.use('/api/v3058', require('./src/routes/v3058'));
	router.use('/api/v3059', require('./src/routes/v3059'));
	router.use('/api/v3060', require('./src/routes/v3060'));
	router.use('/api/v3061', require('./src/routes/v3061'));
	router.use('/api/v3062', require('./src/routes/v3062'));
	router.use('/api/v3063', require('./src/routes/v3063'));
	router.use('/api/v3064', require('./src/routes/v3064'));
	router.use('/api/v3065', require('./src/routes/v3065'));
	router.use('/api/v3066', require('./src/routes/v3066'));
	router.use('/api/v3067', require('./src/routes/v3067'));
	router.use('/api/v3068', require('./src/routes/v3068'));
	router.use('/api/v3069', require('./src/routes/v3069'));
	router.use('/api/v3070', require('./src/routes/v3070'));
	router.use('/api/v3071', require('./src/routes/v3071'));
	router.use('/api/v3072', require('./src/routes/v3072'));
	router.use('/api/v3073', require('./src/routes/v3073'));
	router.use('/api/v3074', require('./src/routes/v3074'));
	router.use('/api/v3075', require('./src/routes/v3075'));
	router.use('/api/v3076', require('./src/routes/v3076'));
	router.use('/api/v3077', require('./src/routes/v3077'));
	router.use('/api/v3078', require('./src/routes/v3078'));
	router.use('/api/v3079', require('./src/routes/v3079'));
	router.use('/api/v3080', require('./src/routes/v3080'));
	router.use('/api/v3081', require('./src/routes/v3081'));
	router.use('/api/v3082', require('./src/routes/v3082'));
	router.use('/api/v3083', require('./src/routes/v3083'));
	router.use('/api/v3084', require('./src/routes/v3084'));
	router.use('/api/v3085', require('./src/routes/v3085'));
	router.use('/api/v3086', require('./src/routes/v3086'));
	router.use('/api/v3087', require('./src/routes/v3087'));
	router.use('/api/v3088', require('./src/routes/v3088'));
	router.use('/api/v3089', require('./src/routes/v3089'));
	router.use('/api/v3090', require('./src/routes/v3090'));
	router.use('/api/v3091', require('./src/routes/v3091'));
	router.use('/api/v3092', require('./src/routes/v3092'));
	router.use('/api/v3093', require('./src/routes/v3093'));
	router.use('/api/v3094', require('./src/routes/v3094'));
	router.use('/api/v3095', require('./src/routes/v3095'));
	router.use('/api/v3096', require('./src/routes/v3096'));
	router.use('/api/v3097', require('./src/routes/v3097'));
	router.use('/api/v3098', require('./src/routes/v3098'));
	router.use('/api/v3099', require('./src/routes/v3099'));
	router.use('/api/v3100', require('./src/routes/v3100'));
	router.use('/api/v3101', require('./src/routes/v3101'));
	router.use('/api/v3102', require('./src/routes/v3102'));
	router.use('/api/v3103', require('./src/routes/v3103'));
	router.use('/api/v3104', require('./src/routes/v3104'));
	router.use('/api/v3105', require('./src/routes/v3105'));
	router.use('/api/v3106', require('./src/routes/v3106'));
	router.use('/api/v3107', require('./src/routes/v3107'));
	router.use('/api/v3108', require('./src/routes/v3108'));
	router.use('/api/v3109', require('./src/routes/v3109'));
	router.use('/api/v3110', require('./src/routes/v3110'));
	router.use('/api/v3111', require('./src/routes/v3111'));
	router.use('/api/v3112', require('./src/routes/v3112'));
	router.use('/api/v3113', require('./src/routes/v3113'));
	router.use('/api/v3114', require('./src/routes/v3114'));
	router.use('/api/v3115', require('./src/routes/v3115'));
	router.use('/api/v3116', require('./src/routes/v3116'));
	router.use('/api/v3117', require('./src/routes/v3117'));
	router.use('/api/v3118', require('./src/routes/v3118'));
	router.use('/api/v3119', require('./src/routes/v3119'));
	router.use('/api/v3120', require('./src/routes/v3120'));
	router.use('/api/v3121', require('./src/routes/v3121'));
	router.use('/api/v3122', require('./src/routes/v3122'));
	router.use('/api/v3123', require('./src/routes/v3123'));
	router.use('/api/v3124', require('./src/routes/v3124'));
	router.use('/api/v3125', require('./src/routes/v3125'));
	router.use('/api/v3126', require('./src/routes/v3126'));
	router.use('/api/v3127', require('./src/routes/v3127'));
	router.use('/api/v3128', require('./src/routes/v3128'));
	router.use('/api/v3129', require('./src/routes/v3129'));
	router.use('/api/v3130', require('./src/routes/v3130'));
	router.use('/api/v3131', require('./src/routes/v3131'));
	router.use('/api/v3132', require('./src/routes/v3132'));
	router.use('/api/v3133', require('./src/routes/v3133'));
	router.use('/api/v3134', require('./src/routes/v3134'));
	router.use('/api/v3135', require('./src/routes/v3135'));
	router.use('/api/v3136', require('./src/routes/v3136'));
	router.use('/api/v3137', require('./src/routes/v3137'));
	router.use('/api/v3138', require('./src/routes/v3138'));
	router.use('/api/v3139', require('./src/routes/v3139'));
	router.use('/api/v3140', require('./src/routes/v3140'));
	router.use('/api/v3141', require('./src/routes/v3141'));
	router.use('/api/v3142', require('./src/routes/v3142'));
	router.use('/api/v3143', require('./src/routes/v3143'));
	router.use('/api/v3144', require('./src/routes/v3144'));
	router.use('/api/v3145', require('./src/routes/v3145'));
	router.use('/api/v3146', require('./src/routes/v3146'));
	router.use('/api/v3147', require('./src/routes/v3147'));
	router.use('/api/v3148', require('./src/routes/v3148'));
	router.use('/api/v3149', require('./src/routes/v3149'));
	router.use('/api/v3150', require('./src/routes/v3150'));
	router.use('/api/v3151', require('./src/routes/v3151'));
	router.use('/api/v3152', require('./src/routes/v3152'));
	router.use('/api/v3153', require('./src/routes/v3153'));
	router.use('/api/v3154', require('./src/routes/v3154'));
	router.use('/api/v3155', require('./src/routes/v3155'));
	router.use('/api/v3156', require('./src/routes/v3156'));
	router.use('/api/v3157', require('./src/routes/v3157'));
	router.use('/api/v3158', require('./src/routes/v3158'));
	router.use('/api/v3159', require('./src/routes/v3159'));
	router.use('/api/v3160', require('./src/routes/v3160'));
	router.use('/api/v3161', require('./src/routes/v3161'));
	router.use('/api/v3162', require('./src/routes/v3162'));
	router.use('/api/v3163', require('./src/routes/v3163'));
	router.use('/api/v3164', require('./src/routes/v3164'));
	router.use('/api/v3165', require('./src/routes/v3165'));
	router.use('/api/v3166', require('./src/routes/v3166'));
	router.use('/api/v3167', require('./src/routes/v3167'));
	router.use('/api/v3168', require('./src/routes/v3168'));
	router.use('/api/v3169', require('./src/routes/v3169'));
	router.use('/api/v3170', require('./src/routes/v3170'));
	router.use('/api/v3171', require('./src/routes/v3171'));
	router.use('/api/v3172', require('./src/routes/v3172'));
	router.use('/api/v3173', require('./src/routes/v3173'));
	router.use('/api/v3174', require('./src/routes/v3174'));
	router.use('/api/v3175', require('./src/routes/v3175'));
	router.use('/api/v3176', require('./src/routes/v3176'));
	router.use('/api/v3177', require('./src/routes/v3177'));
	router.use('/api/v3178', require('./src/routes/v3178'));
	router.use('/api/v3179', require('./src/routes/v3179'));
	router.use('/api/v3180', require('./src/routes/v3180'));
	router.use('/api/v3181', require('./src/routes/v3181'));
	router.use('/api/v3182', require('./src/routes/v3182'));
	router.use('/api/v3183', require('./src/routes/v3183'));
	router.use('/api/v3184', require('./src/routes/v3184'));
	router.use('/api/v3185', require('./src/routes/v3185'));
	router.use('/api/v3186', require('./src/routes/v3186'));
	router.use('/api/v3187', require('./src/routes/v3187'));
	router.use('/api/v3188', require('./src/routes/v3188'));
	router.use('/api/v3189', require('./src/routes/v3189'));
	router.use('/api/v3190', require('./src/routes/v3190'));
	router.use('/api/v3191', require('./src/routes/v3191'));
	router.use('/api/v3192', require('./src/routes/v3192'));
	router.use('/api/v3193', require('./src/routes/v3193'));
	router.use('/api/v3194', require('./src/routes/v3194'));
	router.use('/api/v3195', require('./src/routes/v3195'));
	router.use('/api/v3196', require('./src/routes/v3196'));
	router.use('/api/v3197', require('./src/routes/v3197'));
	router.use('/api/v3198', require('./src/routes/v3198'));
	router.use('/api/v3199', require('./src/routes/v3199'));
	router.use('/api/v3200', require('./src/routes/v3200'));
	router.use('/api/v3201', require('./src/routes/v3201'));
	router.use('/api/v3202', require('./src/routes/v3202'));
	router.use('/api/v3203', require('./src/routes/v3203'));
	router.use('/api/v3204', require('./src/routes/v3204'));
	router.use('/api/v3205', require('./src/routes/v3205'));
	router.use('/api/v3206', require('./src/routes/v3206'));
	router.use('/api/v3207', require('./src/routes/v3207'));
	router.use('/api/v3208', require('./src/routes/v3208'));
	router.use('/api/v3209', require('./src/routes/v3209'));
	router.use('/api/v3210', require('./src/routes/v3210'));
	router.use('/api/v3211', require('./src/routes/v3211'));
	router.use('/api/v3212', require('./src/routes/v3212'));
	router.use('/api/v3213', require('./src/routes/v3213'));
	router.use('/api/v3214', require('./src/routes/v3214'));
	router.use('/api/v3215', require('./src/routes/v3215'));
	router.use('/api/v3216', require('./src/routes/v3216'));
	router.use('/api/v3217', require('./src/routes/v3217'));
	router.use('/api/v3218', require('./src/routes/v3218'));
	router.use('/api/v3219', require('./src/routes/v3219'));
	router.use('/api/v3220', require('./src/routes/v3220'));
	router.use('/api/v3221', require('./src/routes/v3221'));
	router.use('/api/v3222', require('./src/routes/v3222'));
	router.use('/api/v3223', require('./src/routes/v3223'));
	router.use('/api/v3224', require('./src/routes/v3224'));
	router.use('/api/v3225', require('./src/routes/v3225'));
	router.use('/api/v3226', require('./src/routes/v3226'));
	router.use('/api/v3227', require('./src/routes/v3227'));
	router.use('/api/v3228', require('./src/routes/v3228'));
	router.use('/api/v3229', require('./src/routes/v3229'));
	router.use('/api/v3230', require('./src/routes/v3230'));
	router.use('/api/v3231', require('./src/routes/v3231'));
	router.use('/api/v3232', require('./src/routes/v3232'));
	router.use('/api/v3233', require('./src/routes/v3233'));
	router.use('/api/v3234', require('./src/routes/v3234'));
	router.use('/api/v3235', require('./src/routes/v3235'));
	router.use('/api/v3236', require('./src/routes/v3236'));
	router.use('/api/v3237', require('./src/routes/v3237'));
	router.use('/api/v3238', require('./src/routes/v3238'));
	router.use('/api/v3239', require('./src/routes/v3239'));
	router.use('/api/v3240', require('./src/routes/v3240'));
	router.use('/api/v3241', require('./src/routes/v3241'));
	router.use('/api/v3242', require('./src/routes/v3242'));
	router.use('/api/v3243', require('./src/routes/v3243'));
	router.use('/api/v3244', require('./src/routes/v3244'));
	router.use('/api/v3245', require('./src/routes/v3245'));
	router.use('/api/v3246', require('./src/routes/v3246'));
	router.use('/api/v3247', require('./src/routes/v3247'));
	router.use('/api/v3248', require('./src/routes/v3248'));
	router.use('/api/v3249', require('./src/routes/v3249'));
	router.use('/api/v3250', require('./src/routes/v3250'));
	router.use('/api/v3251', require('./src/routes/v3251'));
	router.use('/api/v3252', require('./src/routes/v3252'));
	router.use('/api/v3253', require('./src/routes/v3253'));
	router.use('/api/v3254', require('./src/routes/v3254'));
	router.use('/api/v3255', require('./src/routes/v3255'));
	router.use('/api/v3256', require('./src/routes/v3256'));
	router.use('/api/v3257', require('./src/routes/v3257'));
	router.use('/api/v3258', require('./src/routes/v3258'));
	router.use('/api/v3259', require('./src/routes/v3259'));
	router.use('/api/v3260', require('./src/routes/v3260'));
	router.use('/api/v3261', require('./src/routes/v3261'));
	router.use('/api/v3262', require('./src/routes/v3262'));
	router.use('/api/v3263', require('./src/routes/v3263'));
	router.use('/api/v3264', require('./src/routes/v3264'));
	router.use('/api/v3265', require('./src/routes/v3265'));
	router.use('/api/v3266', require('./src/routes/v3266'));
	router.use('/api/v3267', require('./src/routes/v3267'));
	router.use('/api/v3268', require('./src/routes/v3268'));
	router.use('/api/v3269', require('./src/routes/v3269'));
	router.use('/api/v3270', require('./src/routes/v3270'));
	router.use('/api/v3271', require('./src/routes/v3271'));
	router.use('/api/v3272', require('./src/routes/v3272'));
	router.use('/api/v3273', require('./src/routes/v3273'));
	router.use('/api/v3274', require('./src/routes/v3274'));
	router.use('/api/v3275', require('./src/routes/v3275'));
	router.use('/api/v3276', require('./src/routes/v3276'));
	router.use('/api/v3277', require('./src/routes/v3277'));
	router.use('/api/v3278', require('./src/routes/v3278'));
	router.use('/api/v3279', require('./src/routes/v3279'));
	router.use('/api/v3280', require('./src/routes/v3280'));
	router.use('/api/v3281', require('./src/routes/v3281'));
	router.use('/api/v3282', require('./src/routes/v3282'));
	router.use('/api/v3283', require('./src/routes/v3283'));
	router.use('/api/v3284', require('./src/routes/v3284'));
	router.use('/api/v3285', require('./src/routes/v3285'));
	router.use('/api/v3286', require('./src/routes/v3286'));
	router.use('/api/v3287', require('./src/routes/v3287'));
	router.use('/api/v3288', require('./src/routes/v3288'));
	router.use('/api/v3289', require('./src/routes/v3289'));
	router.use('/api/v3290', require('./src/routes/v3290'));
	router.use('/api/v3291', require('./src/routes/v3291'));
	router.use('/api/v3292', require('./src/routes/v3292'));
	router.use('/api/v3293', require('./src/routes/v3293'));
	router.use('/api/v3294', require('./src/routes/v3294'));
	router.use('/api/v3295', require('./src/routes/v3295'));
	router.use('/api/v3296', require('./src/routes/v3296'));
	router.use('/api/v3297', require('./src/routes/v3297'));
	router.use('/api/v3298', require('./src/routes/v3298'));
	router.use('/api/v3299', require('./src/routes/v3299'));
	router.use('/api/v3300', require('./src/routes/v3300'));
	router.use('/api/v3301', require('./src/routes/v3301'));
	router.use('/api/v3302', require('./src/routes/v3302'));
	router.use('/api/v3303', require('./src/routes/v3303'));
	router.use('/api/v3304', require('./src/routes/v3304'));
	router.use('/api/v3305', require('./src/routes/v3305'));
	router.use('/api/v3306', require('./src/routes/v3306'));
	router.use('/api/v3307', require('./src/routes/v3307'));
	router.use('/api/v3308', require('./src/routes/v3308'));
	router.use('/api/v3309', require('./src/routes/v3309'));
	router.use('/api/v3310', require('./src/routes/v3310'));
	router.use('/api/v3311', require('./src/routes/v3311'));
	router.use('/api/v3312', require('./src/routes/v3312'));
	router.use('/api/v3313', require('./src/routes/v3313'));
	router.use('/api/v3314', require('./src/routes/v3314'));
	router.use('/api/v3315', require('./src/routes/v3315'));
	router.use('/api/v3316', require('./src/routes/v3316'));
	router.use('/api/v3317', require('./src/routes/v3317'));
	router.use('/api/v3318', require('./src/routes/v3318'));
	router.use('/api/v3319', require('./src/routes/v3319'));
	router.use('/api/v3320', require('./src/routes/v3320'));
	router.use('/api/v3321', require('./src/routes/v3321'));
	router.use('/api/v3322', require('./src/routes/v3322'));
	router.use('/api/v3323', require('./src/routes/v3323'));
	router.use('/api/v3324', require('./src/routes/v3324'));
	router.use('/api/v3325', require('./src/routes/v3325'));
	router.use('/api/v3326', require('./src/routes/v3326'));
	router.use('/api/v3327', require('./src/routes/v3327'));
	router.use('/api/v3328', require('./src/routes/v3328'));
	router.use('/api/v3329', require('./src/routes/v3329'));
	router.use('/api/v3330', require('./src/routes/v3330'));
	router.use('/api/v3331', require('./src/routes/v3331'));
	router.use('/api/v3332', require('./src/routes/v3332'));
	router.use('/api/v3333', require('./src/routes/v3333'));
	router.use('/api/v3334', require('./src/routes/v3334'));
	router.use('/api/v3335', require('./src/routes/v3335'));
	router.use('/api/v3336', require('./src/routes/v3336'));
	router.use('/api/v3337', require('./src/routes/v3337'));
	router.use('/api/v3338', require('./src/routes/v3338'));
	router.use('/api/v3339', require('./src/routes/v3339'));
	router.use('/api/v3340', require('./src/routes/v3340'));
	router.use('/api/v3341', require('./src/routes/v3341'));
	router.use('/api/v3342', require('./src/routes/v3342'));
	router.use('/api/v3343', require('./src/routes/v3343'));
	router.use('/api/v3344', require('./src/routes/v3344'));
	router.use('/api/v3345', require('./src/routes/v3345'));
	router.use('/api/v3346', require('./src/routes/v3346'));
	router.use('/api/v3347', require('./src/routes/v3347'));
	router.use('/api/v3348', require('./src/routes/v3348'));
	router.use('/api/v3349', require('./src/routes/v3349'));
	router.use('/api/v3350', require('./src/routes/v3350'));
	router.use('/api/v3351', require('./src/routes/v3351'));
	router.use('/api/v3352', require('./src/routes/v3352'));
	router.use('/api/v3353', require('./src/routes/v3353'));
	router.use('/api/v3354', require('./src/routes/v3354'));
	router.use('/api/v3355', require('./src/routes/v3355'));
	router.use('/api/v3356', require('./src/routes/v3356'));
	router.use('/api/v3357', require('./src/routes/v3357'));
	router.use('/api/v3358', require('./src/routes/v3358'));
	router.use('/api/v3359', require('./src/routes/v3359'));
	router.use('/api/v3360', require('./src/routes/v3360'));
	router.use('/api/v3361', require('./src/routes/v3361'));
	router.use('/api/v3362', require('./src/routes/v3362'));
	router.use('/api/v3363', require('./src/routes/v3363'));
	router.use('/api/v3364', require('./src/routes/v3364'));
	router.use('/api/v3365', require('./src/routes/v3365'));
	router.use('/api/v3366', require('./src/routes/v3366'));
	router.use('/api/v3367', require('./src/routes/v3367'));
	router.use('/api/v3368', require('./src/routes/v3368'));
	router.use('/api/v3369', require('./src/routes/v3369'));
	router.use('/api/v3370', require('./src/routes/v3370'));
	router.use('/api/v3371', require('./src/routes/v3371'));
	router.use('/api/v3372', require('./src/routes/v3372'));
	router.use('/api/v3373', require('./src/routes/v3373'));
	router.use('/api/v3374', require('./src/routes/v3374'));
	router.use('/api/v3375', require('./src/routes/v3375'));
	router.use('/api/v3376', require('./src/routes/v3376'));
	router.use('/api/v3377', require('./src/routes/v3377'));
	router.use('/api/v3378', require('./src/routes/v3378'));
	router.use('/api/v3379', require('./src/routes/v3379'));
	router.use('/api/v3380', require('./src/routes/v3380'));
	router.use('/api/v3381', require('./src/routes/v3381'));
	router.use('/api/v3382', require('./src/routes/v3382'));
	router.use('/api/v3383', require('./src/routes/v3383'));
	router.use('/api/v3384', require('./src/routes/v3384'));
	router.use('/api/v3385', require('./src/routes/v3385'));
	router.use('/api/v3386', require('./src/routes/v3386'));
	router.use('/api/v3387', require('./src/routes/v3387'));
	router.use('/api/v3388', require('./src/routes/v3388'));
	router.use('/api/v3389', require('./src/routes/v3389'));
	router.use('/api/v3390', require('./src/routes/v3390'));
	router.use('/api/v3391', require('./src/routes/v3391'));
	router.use('/api/v3392', require('./src/routes/v3392'));
	router.use('/api/v3393', require('./src/routes/v3393'));
	router.use('/api/v3394', require('./src/routes/v3394'));
	router.use('/api/v3395', require('./