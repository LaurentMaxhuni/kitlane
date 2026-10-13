import {Pool} from 'pg';
import {attachDatabasePool} from '@neon/functions';
import {config} from './config.js';
import {createApp} from './app.js';
import {html,script} from './assets.js';

if (!html || !script) throw new Error('Build the account service before deployment.');
const pool=new Pool({connectionString:process.env.DATABASE_URL,max:5,connectionTimeoutMillis:5000,query_timeout:10000});
attachDatabasePool(pool,{onUnexpectedError:()=>{/* Do not log connection metadata. */}});
export default createApp(pool,config(),{html,script});
