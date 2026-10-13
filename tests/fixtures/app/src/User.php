<?php
namespace App\Models;

use App\Contracts\HasName;
use App\Support\Str as S;

final class User extends Model implements HasName
{
    public const ROLE = 'user';
    private string $name;
    public static function create(string $name): self { return new static(); }
    public function getName(): string { return S::upper($this->name); }
}

function helper(): void {}
interface HasName {}
trait Greets {}
enum Status: string { case Active = 'a'; }
$u = User::create('x');
$u->getName();
helper();
\App\Models\helper();
echo User::ROLE;
