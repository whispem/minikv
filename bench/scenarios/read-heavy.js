import http from 'k6/http';
import { check, fail, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const readSuccess = new Rate('read_success');
const readLatency = new Trend('read_latency');
const writeSuccess = new Rate('write_success');

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:5000';
const OBJECT_SIZE = parseInt(__ENV.OBJECT_SIZE || '1048576');
const KEY_COUNT = parseInt(__ENV.KEY_COUNT || '1000');
const READ_RATIO = 0.9;

export let options = {
    stages: [
        { duration: '30s', target: 20 },
        { duration: '2m', target: 100 },
        { duration: '30s', target: 0 },
    ],
    // setup() writes KEY_COUNT objects before the run.
    setupTimeout: '10m',
    thresholds: {
        'read_success': ['rate>0.99'],
        'read_latency': ['p(95)<100'],
    },
};

function generateData(size) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < size; i++) {
        result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
}

// Writes the keys that the run reads, and stops if one cannot be written.
export function setup() {
    const data = generateData(OBJECT_SIZE);
    const keys = [];
    for (let i = 0; i < KEY_COUNT; i++) {
        const key = `read-heavy-key-${i}`;
        const res = http.put(`${BASE_URL}/${key}`, data);
        if (res.status !== 200) {
            fail(`setup: PUT ${key} answered ${res.status}`);
        }
        keys.push(key);
    }
    return { keys, size: data.length };
}

export default function (data) {
    const key = data.keys[Math.floor(Math.random() * data.keys.length)];

    if (Math.random() < READ_RATIO) {
        const start = Date.now();
        const res = http.get(`${BASE_URL}/${key}`);
        const duration = Date.now() - start;

        // A read succeeds with 200 and the whole value; anything else fails.
        const ok = res.status === 200 && res.body.length === data.size;
        readSuccess.add(ok);
        readLatency.add(duration);

        check(res, {
            'read ok': () => ok,
        });
    } else {
        const res = http.put(`${BASE_URL}/${key}`, generateData(OBJECT_SIZE));
        writeSuccess.add(res.status === 200);
        check(res, {
            'write ok': (r) => r.status === 200,
        });
    }

    sleep(0.05);
}
