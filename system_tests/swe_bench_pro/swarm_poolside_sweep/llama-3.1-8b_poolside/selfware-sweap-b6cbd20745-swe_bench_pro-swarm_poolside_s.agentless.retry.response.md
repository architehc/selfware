### FILE: src/prestart.js
<<<<<<< 
const pkg = require('../package.json');
const { paths } = require('./constants');
>>>>>>> 
const pkg = require('../package.json');
const { paths } = require('./constants');
const { loadConfig } = require('./loadConfig');

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
exports.setupWinston = setupWinston;
exports.loadConfig = loadConfig;
exports.versionCheck = versionCheck;
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
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
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
if (testDbConfig.database === productionDbConfig.database &&
	testDbConfig.host === productionDbConfig.host &&
	testDbConfig.port === productionDbConfig.port) {
	const errorText = 'test_database has the same config as production db';
	winston.error(errorText);
	throw new Error(errorText);
}
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
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
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
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
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
async function setupDefaultConfigs(meta) {
	winston.info('Populating database with default configs, if not already set...\n');

	const defaults = require(path.join(nconf.get('base_dir'), 'install/data/defaults.json'));
	defaults.eventLoopCheckEnabled = 0;
	defaults.minimumPasswordStrength = 0;
	await meta.configs.setOnEmpty(defaults);
}
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
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
>>>>>>> 

### FILE: test/mocks/databasemock.js
<<<<<<< 
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

### FILE: Dockerfile
<<<<<<< 
FROM --platform=$BUILDPLATFORM node:lts as npm
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
FROM node:lts as rebuild
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
ARG BUILDPLATFORM
ARG TARGETPLATFORM
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
RUN mkdir -p /usr/src/build && \
    chown -R node:node /usr/src/build
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
WORKDIR /usr/src/build
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
COPY --from=npm /usr/src/build /usr/src/build
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
RUN if [ $BUILDPLATFORM != $TARGETPLATFORM ]; then \
    npm rebuild && \
    npm cache clean --force; fi
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
FROM node:lts-slim as run
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
ARG NODE_ENV
ENV NODE_ENV=$NODE_ENV \
    daemon=false \
    silent=false
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
RUN mkdir -p /usr/src/app && \
    chown -R node:node /usr/src/app
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
COPY --chown=node:node --from=rebuild /usr/src/build /usr/src/app
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
WORKDIR /usr/src/app
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
USER node
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
COPY --chown=node:node . /usr/src/app
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
EXPOSE 4567
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
VOLUME ["/usr/src/app/node_modules", "/usr/src/app/build", "/usr/src/app/public/uploads", "/opt/config"]
>>>>>>> 

### FILE: Dockerfile
<<<<<<< 
ENTRYPOINT ["./install/docker/entrypoint.sh"]
>>>>>>> 

### FILE: types/database/hash.d.ts
<<<<<<< 
export interface Hash {
  decrObjectField(
    key: string | string[],
    field: string,
  ): Promise<number | number[]>

  deleteObjectField(key: string, field: string): Promise<void>

  deleteObjectFields(key: string, fields: string[]): Promise<void>

  getObject(key: string, fields: string[]): Promise<object>

  getObjectField(key: string, field: string): Promise<any>

  getObjectFields(key: string, fields: string[]): Promise<Record<string, any>>

  getObjectKeys(key: string): Promise<string[]>

  getObjectValues(key: string): Promise<any[]>

  getObjects(keys: string[], fields: string[]): Promise<any[]>

  getObjectsFields(
    keys: string[],
    fields: string[],
  ): Promise<Record<string, any>[]>

  incrObjectField(
    key: string | string[],
    field: string,
  ): Promise<number | number[]>

  incrObjectFieldBy(
    key: string | string[],
    field: string,
    value: number,
  ): Promise<number | number[]>

  incrObjectFieldByBulk(
    data: [key: string, batch: Record<string, number>][],
  ): Promise<void>

  isObjectField(key: string, field: string): Promise<boolean>

  isObjectFields(key: string, fields: string[]): Promise<boolean[]>

  setObject(key: string | string[], data: Record<string, any>): Promise<void>

  setObjectBulk(args: [key: string, data: Record<string, any>][]): Promise<void>

  setObjectField(
    key: string | string[],
    field: string,
    value: any,
  ): Promise<void>
}
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const file = require('./src/file');
const pkg = require('./package.json');
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const pathToConfig = path.resolve(__dirname, process.env.CONFIG || 'config.json');
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const outputLogFilePath = path.join(__dirname, nconf.get('logFile') || 'logs/output.log');
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const logDir = path.dirname(outputLogFilePath);
if (!fs.existsSync(logDir)) {
	mkdirp.sync(path.dirname(outputLogFilePath));
}
>>>>>>> 

### FILE: loader.js
<<<<<<< 
const output = logrotate({ file: outputLogFilePath, size: '1m', keep: 3, compress: true });
const silent = nconf.get('silent') === 'false' ? false : nconf.get('silent') !== false;
let numProcs;
const workers = [];
const Loader = {};
const appPath = path.join(__dirname, 'app.js');
>>>>>>> 

### FILE: loader.js
<<<<<<< 
Loader.init = function () {
	if (silent) {
		console.log = (...args) => {
			output.write(`${args.join(' ')}\n`);
		};
	}

	process.on('SIGHUP', Loader.restart);
	process.on('SIGTERM', Loader.stop);
};
>>>>>>> 

### FILE: loader.js
<<<<<<< 
Loader.displayStartupMessages = function () {
	console.log('');
	console.log(`NodeBB v${pkg.version} Copyright (C) 2013-${(new Date()).getFullYear()} NodeBB Inc.`);
	console.log('This program comes with ABSOLUTELY NO WARRANTY.');
	console.log('This is free software, and you are welcome to redistribute it under certain conditions.');
	console.log('For the full license, please visit: http://www.gnu.org/copyleft/gpl.html');
	console.log('');
};
>>>>>>> 

### FILE: loader.js
<<<<<<< 
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
>>>>>>> 

### FILE: loader.js
<<<<<<< 
Loader.start = function () {
	numProcs = getPorts().length;
	console.log(`Clustering enabled: Spinning up ${numProcs} process(es).\n`);

	for (let x = 0; x < numProcs; x += 1) {
		forkWorker(x, x === 0);
	}
};
>>>>>>> 

### FILE: loader.js
<<<<<<< 
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
>>>>>>> 

### FILE: loader.js
<<<<<<< 
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
>>>>>>> 

### FILE: loader.js
<<<<<<< 
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
>>>>>>> 

### FILE: loader.js
<<<<<<< 
Loader.stop = function () {
	killWorkers();

	// Clean up the pidfile
	if (nconf.get('daemon') !== 'false' && nconf.get('daemon') !== false) {
		fs.unlinkSync(pidFilePath);
	}
};
>>>>>>> 

### FILE: loader.js
<<<<<<< 
function killWorkers() {
	workers.forEach((worker) => {
		worker.suicide = true;
		worker.kill();
	});
}
>>>>>>> 

### FILE: loader.js
<<<<<<< 
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

### FILE: src/controllers/well-known.js
<<<<<<< 
// new file
>>>>>>>