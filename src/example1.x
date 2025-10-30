int fact(int n) {
    int res = 1;
    for (int i = 1; i <= n; i = i + 1) {
        res *= i;
    }
    return res;
}

int main() {
    int a = 3;
    int b = 0;
    if (a > 2) {
        b = fact(a);
    } else {
        b = 42;
    }
    while (b > 0) {
        b -= 2;
        if (b == 10) break;
        if (b == 8) continue;
    }
    return b;
}
