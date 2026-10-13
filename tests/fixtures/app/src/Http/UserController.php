<?php

namespace App\Http;

use App\Models\User;
use App\Models\Status;
use App\Support\Str as S;
use function App\Support\current_user;

class UserController
{
    public function show(User $user): string
    {
        $user->greet();
        $fresh = User::create();
        $fresh->save();
        $city = $user->address()->format();
        $status = Status::Active;
        current_user()->getName();
        return S::upper($user->getName()) . User::ROLE;
    }
}
